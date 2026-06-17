import Foundation
import XCTest

@testable import AppleVzRunnerCore

#if canImport(Virtualization)
import Virtualization
#endif

final class AppleVzRunnerCoreTests: XCTestCase {
  func testDecodesReadyHandoffFromRustShape() throws {
    let handoff = try AppleVzHandoffValidator.decode(Data(readyHandoffJSON.utf8))

    XCTAssertEqual(handoff.backend, "apple-virtualization-framework")
    XCTAssertEqual(handoff.vmName, "fast-linux")
    XCTAssertEqual(handoff.launchSpecPath, "/tmp/fast.vmbridge/metadata/apple-vz-launch.json")
    XCTAssertEqual(handoff.guest.arch, "arm64")
    XCTAssertEqual(handoff.bootMode, "linux-installer")
    XCTAssertEqual(handoff.disk.format, "qcow2")
    XCTAssertEqual(handoff.resources.memory, "4096")
    XCTAssertEqual(handoff.resources.cpu, "2")
    XCTAssertTrue(handoff.integration.virtiofs)
    XCTAssertTrue(handoff.readiness.ready)
  }

  func testValidatesReadyHandoffBoundary() throws {
    let handoff = try AppleVzHandoffValidator.decode(Data(readyHandoffJSON.utf8))

    let validation = try AppleVzHandoffValidator.validate(handoff)

    XCTAssertEqual(validation.vmName, "fast-linux")
    XCTAssertEqual(validation.backend, "apple-virtualization-framework")
    XCTAssertEqual(validation.bootMode, "linux-installer")
    XCTAssertEqual(validation.diskPath, "/tmp/fast.vmbridge/disks/root.qcow2")
    XCTAssertEqual(validation.memoryMiB, 4096)
    XCTAssertEqual(validation.cpuCount, 2)
    XCTAssertEqual(validation.virtualizationFrameworkLinked, AppleVzHandoffValidator.virtualizationFrameworkLinked())
  }

  func testBuildsConfigurationPlanFromReadyHandoff() throws {
    let handoff = try AppleVzHandoffValidator.decode(Data(readyHandoffJSON.utf8))

    let plan = try AppleVzHandoffValidator.configurationPlan(for: handoff)

    XCTAssertEqual(plan.vmName, "fast-linux")
    XCTAssertEqual(plan.bootMode, "linux-installer")
    XCTAssertEqual(plan.bootLoader, "efi")
    XCTAssertEqual(plan.platform, "generic")
    XCTAssertEqual(plan.diskAttachment, "disk-image-qcow2")
    XCTAssertEqual(plan.networkAttachment, "nat")
    XCTAssertEqual(plan.memoryBytes, 4096 * 1024 * 1024)
    XCTAssertEqual(plan.cpuCount, 2)
    XCTAssertTrue(plan.entropyDevice)
    XCTAssertTrue(plan.balloonDevice)
    XCTAssertEqual(plan.serialLogPath, "/tmp/fast.vmbridge/logs/serial.log")
  }

  func testBuildsLinuxKernelRawConfigurationPlanBeforeLaunchBoundary() throws {
    var handoff = try AppleVzHandoffValidator.decode(Data(readyHandoffJSON.utf8))
    handoff.bootMode = "linux-kernel"
    handoff.disk.format = "raw"
    handoff.resources.balloonDevice = false

    let plan = try AppleVzHandoffValidator.configurationPlan(for: handoff)

    XCTAssertEqual(plan.bootMode, "linux-kernel")
    XCTAssertEqual(plan.bootLoader, "linux-kernel")
    XCTAssertEqual(plan.diskAttachment, "disk-image-raw")
    XCTAssertEqual(plan.networkAttachment, "nat")
    XCTAssertFalse(plan.balloonDevice)
  }

  func testRejectsInvalidResourcesBeforeLaunchConstruction() throws {
    var invalidMemory = try AppleVzHandoffValidator.decode(Data(readyHandoffJSON.utf8))
    invalidMemory.resources.memory = "four-gb"

    XCTAssertThrowsError(try AppleVzHandoffValidator.validate(invalidMemory)) { error in
      XCTAssertEqual(error as? AppleVzRunnerError, .invalidMemory("four-gb"))
    }

    var invalidCPU = try AppleVzHandoffValidator.decode(Data(readyHandoffJSON.utf8))
    invalidCPU.resources.cpu = "0"

    XCTAssertThrowsError(try AppleVzHandoffValidator.validate(invalidCPU)) { error in
      XCTAssertEqual(error as? AppleVzRunnerError, .invalidCPU("0"))
    }
  }

  func testRejectsBlockedHandoffBeforeLaunchConstruction() throws {
    let handoff = try AppleVzHandoffValidator.decode(Data(blockedHandoffJSON.utf8))

    XCTAssertThrowsError(try AppleVzHandoffValidator.validate(handoff)) { error in
      XCTAssertEqual(
        error as? AppleVzRunnerError,
        .notReady([
          AppleVzReadinessBlocker(
            code: "missing-primary-disk",
            message: "Primary disk is missing.",
            path: "/tmp/fast.vmbridge/disks/root.qcow2",
            capability: nil,
          )
        ])
      )
    }
  }

#if canImport(Virtualization)
  func testBuildsLinuxKernelConfigurationFromReadyLaunchSpec() throws {
    var spec = try decodeLaunchSpec(readyLinuxKernelLaunchSpecJSON)
    let diskURL = try makeTemporaryRawDisk()
    defer { try? FileManager.default.removeItem(at: diskURL) }
    spec.disk.path = diskURL.path

    let configuration = try AppleVzConfigurationBuilder.buildLinuxKernelConfiguration(spec: spec)

    XCTAssertEqual(configuration.cpuCount, 4)
    XCTAssertEqual(configuration.memorySize, 8192 * 1024 * 1024)
    XCTAssertEqual(configuration.storageDevices.count, 1)
    XCTAssertEqual(configuration.networkDevices.count, 1)
    XCTAssertEqual(configuration.entropyDevices.count, 1)
    XCTAssertEqual(configuration.memoryBalloonDevices.count, 1)
    XCTAssertTrue(configuration.bootLoader is VZLinuxBootLoader)
    XCTAssertTrue(configuration.networkDevices.first?.attachment is VZNATNetworkDeviceAttachment)
  }

  func testBuildsVirtioSharedDirectoryDeviceWhenRequested() throws {
    var spec = try decodeLaunchSpec(readyLinuxKernelLaunchSpecJSON)
    let diskURL = try makeTemporaryRawDisk()
    defer { try? FileManager.default.removeItem(at: diskURL) }
    spec.disk.path = diskURL.path

    let withShare = try AppleVzConfigurationBuilder.buildLinuxKernelConfiguration(
      spec: spec,
      sharedDirectory: AppleVzSharedDirectorySpec(
        path: FileManager.default.temporaryDirectory.path,
        tag: "share"
      )
    )
    XCTAssertEqual(withShare.directorySharingDevices.count, 1)

    // No share requested -> no directory-sharing device (unchanged default).
    let plain = try AppleVzConfigurationBuilder.buildLinuxKernelConfiguration(spec: spec)
    XCTAssertEqual(plain.directorySharingDevices.count, 0)
  }

  func testRejectsQcow2LinuxKernelLaunchSpec() throws {
    var spec = try decodeLaunchSpec(readyLinuxKernelLaunchSpecJSON)
    spec.disk.format = "qcow2"

    XCTAssertThrowsError(
      try AppleVzConfigurationBuilder.buildLinuxKernelConfiguration(spec: spec)
    ) { error in
      XCTAssertEqual(error as? AppleVzRunnerError, .unsupportedDiskFormat("qcow2"))
    }
  }

  func testRejectsLinuxInstallerLaunchSpec() throws {
    var spec = try decodeLaunchSpec(readyLinuxKernelLaunchSpecJSON)
    spec.boot.mode = "linux-installer"

    XCTAssertThrowsError(
      try AppleVzConfigurationBuilder.buildLinuxKernelConfiguration(spec: spec)
    ) { error in
      XCTAssertEqual(error as? AppleVzRunnerError, .unsupportedBootMode("linux-installer"))
    }
  }

  func testRejectsNonNatLinuxKernelLaunchSpec() throws {
    var spec = try decodeLaunchSpec(readyLinuxKernelLaunchSpecJSON)
    spec.devices.network = "bridged"

    XCTAssertThrowsError(
      try AppleVzConfigurationBuilder.buildLinuxKernelConfiguration(spec: spec)
    ) { error in
      XCTAssertEqual(error as? AppleVzRunnerError, .unsupportedNetwork("bridged"))
    }
  }

  func testRejectsNotReadyLinuxKernelLaunchSpec() throws {
    var spec = try decodeLaunchSpec(readyLinuxKernelLaunchSpecJSON)
    spec.readiness = AppleVzReadinessSpec(
      ready: false,
      blockers: [
        AppleVzReadinessBlocker(
          code: "missing-kernel",
          message: "Kernel image is missing.",
          path: "/tmp/fast.vmbridge/boot/vmlinuz",
          capability: nil
        )
      ]
    )

    XCTAssertThrowsError(
      try AppleVzConfigurationBuilder.buildLinuxKernelConfiguration(spec: spec)
    ) { error in
      XCTAssertEqual(error as? AppleVzRunnerError, .notReady(spec.readiness.blockers))
    }
  }
#endif

  private func decodeLaunchSpec(_ json: String) throws -> AppleVzLaunchSpec {
    try JSONDecoder().decode(AppleVzLaunchSpec.self, from: Data(json.utf8))
  }

  private func makeTemporaryRawDisk() throws -> URL {
    let directory = FileManager.default.temporaryDirectory
      .appendingPathComponent(UUID().uuidString, isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    let url = directory.appendingPathComponent("root.raw")
    FileManager.default.createFile(atPath: url.path, contents: Data(count: 1024 * 1024))
    return url
  }

  private var readyHandoffJSON: String {
    """
    {
      "backend": "apple-virtualization-framework",
      "vm_name": "fast-linux",
      "bundle_path": "/tmp/fast.vmbridge",
      "launch_spec_path": "/tmp/fast.vmbridge/metadata/apple-vz-launch.json",
      "guest": {
        "os": "ubuntu",
        "arch": "arm64"
      },
      "boot_mode": "linux-installer",
      "disk": {
        "path": "/tmp/fast.vmbridge/disks/root.qcow2",
        "format": "qcow2",
        "read_only": false
      },
      "resources": {
        "memory": "4096",
        "cpu": "2",
        "display_fps_cap": "60",
        "rationale": "Automatic balanced policy.",
        "balloon_device": true
      },
      "runner_log_path": "/tmp/fast.vmbridge/logs/lightvm.log",
      "serial_log_path": "/tmp/fast.vmbridge/logs/serial.log",
      "integration": {
        "clipboard": true,
        "dynamic_resolution": true,
        "shared_folders": true,
        "virtiofs": true
      },
      "readiness": {
        "ready": true,
        "blockers": []
      }
    }
    """
  }

  private var blockedHandoffJSON: String {
    """
    {
      "backend": "apple-virtualization-framework",
      "vm_name": "fast-linux",
      "bundle_path": "/tmp/fast.vmbridge",
      "guest": {
        "os": "ubuntu",
        "arch": "arm64"
      },
      "boot_mode": "linux-installer",
      "disk": {
        "path": "/tmp/fast.vmbridge/disks/root.qcow2",
        "format": "qcow2",
        "read_only": false
      },
      "resources": {
        "memory": "4096",
        "cpu": "2",
        "display_fps_cap": "60",
        "rationale": "Automatic balanced policy.",
        "balloon_device": true
      },
      "runner_log_path": "/tmp/fast.vmbridge/logs/lightvm.log",
      "serial_log_path": "/tmp/fast.vmbridge/logs/serial.log",
      "integration": {
        "clipboard": true,
        "dynamic_resolution": true,
        "shared_folders": true,
        "virtiofs": true
      },
      "readiness": {
        "ready": false,
        "blockers": [
          {
            "code": "missing-primary-disk",
            "message": "Primary disk is missing.",
            "path": "/tmp/fast.vmbridge/disks/root.qcow2"
          }
        ]
      }
    }
    """
  }

  private var readyLinuxKernelLaunchSpecJSON: String {
    """
    {
      "vm_name": "fast-linux",
      "bundle_path": "/tmp/fast.vmbridge",
      "guest": {
        "os": "ubuntu",
        "arch": "aarch64"
      },
      "boot": {
        "mode": "linux-kernel",
        "installer_image": null,
        "kernel": {
          "path": "/tmp/fast.vmbridge/boot/vmlinuz",
          "exists": true
        },
        "initrd": {
          "path": "/tmp/fast.vmbridge/boot/initrd",
          "exists": true
        },
        "kernel_command_line": "console=hvc0 root=/dev/vda rw",
        "macos_restore_image": null
      },
      "disk": {
        "path": "/tmp/fast.vmbridge/disks/root.raw",
        "format": "raw",
        "read_only": false
      },
      "resources": {
        "memory": "8192",
        "cpu": "4",
        "display_fps_cap": "60",
        "rationale": "Manual launch policy.",
        "balloon_device": true
      },
      "devices": {
        "entropy_device": true,
        "network": "nat",
        "serial_log_path": "/tmp/fast.vmbridge/logs/serial.log"
      },
      "integration": {
        "clipboard": true,
        "dynamic_resolution": true,
        "shared_folders": true,
        "virtiofs": true
      },
      "logs": {
        "runner_log_path": "/tmp/fast.vmbridge/logs/lightvm.log"
      },
      "readiness": {
        "ready": true,
        "blockers": []
      }
    }
    """
  }
}
