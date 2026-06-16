import Foundation
import XCTest

@testable import AppleVzRunnerCore

final class AppleVzRunnerCommandTests: XCTestCase {
  func testHelpReturnsSuccessWithUsageWithoutReadingInputOrLaunching() {
    let fake = FakeRunnerDependencies(standardInput: "this should not be read")

    let exitCode = AppleVzRunnerCommand.run(
      arguments: ["--help"],
      dependencies: fake.dependencies()
    )

    XCTAssertEqual(exitCode, 0)
    XCTAssertEqual(
      fake.outputLines,
      [
        "usage: AppleVzRunner [--handoff-json PATH] [--validate-only] [--print-config-plan] [--validate-vz-config] [--allow-real-vz-start] [--stop-after-seconds N] [--force-stop-grace-seconds N]"
      ]
    )
    XCTAssertEqual(fake.readStandardInputCallCount, 0)
    XCTAssertTrue(fake.readFilePaths.isEmpty)
    XCTAssertEqual(fake.validateVzConfigurationCallCount, 0)
    XCTAssertEqual(fake.launchVirtualMachineCallCount, 0)
    XCTAssertTrue(fake.errorLines.isEmpty)
  }

  func testValidateOnlyReturnsZeroAfterHandoffValidationWithoutReadingLaunchSpecOrLaunching() {
    let fake = FakeRunnerDependencies(standardInput: readyHandoffJSON(launchSpecPath: nil))

    let exitCode = AppleVzRunnerCommand.run(
      arguments: ["--validate-only"],
      dependencies: fake.dependencies()
    )

    XCTAssertEqual(exitCode, 0)
    XCTAssertTrue(fake.readFilePaths.isEmpty)
    XCTAssertEqual(fake.validateVzConfigurationCallCount, 0)
    XCTAssertEqual(fake.launchVirtualMachineCallCount, 0)
    XCTAssertTrue(fake.outputLines.contains("AppleVzRunner handoff ready"))
    XCTAssertTrue(fake.errorLines.isEmpty)
  }

  func testPrintConfigPlanWritesPlanWithoutReadingLaunchSpecOrLaunching() {
    let fake = FakeRunnerDependencies(standardInput: readyHandoffJSON(launchSpecPath: nil))

    let exitCode = AppleVzRunnerCommand.run(
      arguments: ["--print-config-plan"],
      dependencies: fake.dependencies()
    )

    XCTAssertEqual(exitCode, 0)
    XCTAssertTrue(fake.readFilePaths.isEmpty)
    XCTAssertEqual(fake.validateVzConfigurationCallCount, 0)
    XCTAssertEqual(fake.launchVirtualMachineCallCount, 0)
    XCTAssertTrue(fake.outputLines.contains("Configuration plan:"))
    XCTAssertTrue(fake.outputLines.contains("Boot loader: linux-kernel"))
    XCTAssertTrue(fake.errorLines.isEmpty)
  }

  func testValidateVzConfigReadsLaunchSpecAndValidatesConfigurationWithoutOptInOrLaunching() {
    let launchSpecPath = "/tmp/fast.vmbridge/metadata/apple-vz-launch.json"
    let fake = FakeRunnerDependencies(
      standardInput: readyHandoffJSON(launchSpecPath: launchSpecPath),
      files: [launchSpecPath: readyLinuxKernelLaunchSpecJSON]
    )

    let exitCode = AppleVzRunnerCommand.run(
      arguments: ["--validate-vz-config"],
      dependencies: fake.dependencies()
    )

    XCTAssertEqual(exitCode, 0)
    XCTAssertEqual(fake.readFilePaths, [launchSpecPath])
    XCTAssertEqual(fake.validateVzConfigurationCallCount, 1)
    XCTAssertEqual(fake.launchVirtualMachineCallCount, 0)
    XCTAssertEqual(fake.validatedSpecs.first?.vmName, "fast-linux")
    XCTAssertTrue(fake.outputLines.contains("VZ configuration validation: ready"))
    XCTAssertTrue(fake.errorLines.isEmpty)
  }

  func testValidateVzConfigWithoutLaunchSpecPathReturnsMissingLaunchSpecPathWithoutLaunching() {
    let fake = FakeRunnerDependencies(standardInput: readyHandoffJSON(launchSpecPath: nil))

    let exitCode = AppleVzRunnerCommand.run(
      arguments: ["--validate-vz-config"],
      dependencies: fake.dependencies()
    )

    XCTAssertEqual(exitCode, 1)
    XCTAssertTrue(fake.readFilePaths.isEmpty)
    XCTAssertEqual(fake.validateVzConfigurationCallCount, 0)
    XCTAssertEqual(fake.launchVirtualMachineCallCount, 0)
    XCTAssertEqual(fake.errorLines, ["--validate-vz-config requires handoff launch_spec_path"])
  }

  func testDefaultRunRequiresRealStartOptInBeforeReadingLaunchSpec() {
    let launchSpecPath = "/tmp/fast.vmbridge/metadata/apple-vz-launch.json"
    let fake = FakeRunnerDependencies(
      standardInput: readyHandoffJSON(launchSpecPath: launchSpecPath),
      files: [launchSpecPath: readyLinuxKernelLaunchSpecJSON]
    )

    let exitCode = AppleVzRunnerCommand.run(arguments: [], dependencies: fake.dependencies())

    XCTAssertEqual(exitCode, 1)
    XCTAssertTrue(fake.readFilePaths.isEmpty)
    XCTAssertEqual(fake.validateVzConfigurationCallCount, 0)
    XCTAssertEqual(fake.launchVirtualMachineCallCount, 0)
    XCTAssertEqual(fake.errorLines, ["real Apple VZ start requires --allow-real-vz-start"])
  }

  func testAllowRealVzStartReadsLaunchSpecAndInvokesLauncherOnce() {
    let launchSpecPath = "/tmp/fast.vmbridge/metadata/apple-vz-launch.json"
    let fake = FakeRunnerDependencies(
      standardInput: readyHandoffJSON(launchSpecPath: launchSpecPath),
      files: [launchSpecPath: readyLinuxKernelLaunchSpecJSON]
    )

    let exitCode = AppleVzRunnerCommand.run(
      arguments: ["--allow-real-vz-start"],
      dependencies: fake.dependencies()
    )

    XCTAssertEqual(exitCode, 0)
    XCTAssertEqual(fake.readFilePaths, [launchSpecPath])
    XCTAssertEqual(fake.validateVzConfigurationCallCount, 0)
    XCTAssertEqual(fake.launchVirtualMachineCallCount, 1)
    XCTAssertEqual(fake.launchedSpecs.first?.vmName, "fast-linux")
    XCTAssertEqual(fake.launchOptions.first, AppleVzLaunchOptions())
    XCTAssertTrue(fake.errorLines.isEmpty)
  }

  func testAllowRealVzStartReportsNSErrorLaunchDetails() {
    let launchSpecPath = "/tmp/fast.vmbridge/metadata/apple-vz-launch.json"
    let fake = FakeRunnerDependencies(
      standardInput: readyHandoffJSON(launchSpecPath: launchSpecPath),
      files: [launchSpecPath: readyLinuxKernelLaunchSpecJSON],
      launchError: NSError(
        domain: "VZErrorDomain",
        code: 1,
        userInfo: [
          NSLocalizedDescriptionKey: "The virtual machine failed to start.",
          NSLocalizedFailureReasonErrorKey: "Internal Virtualization error.",
        ]
      )
    )

    let exitCode = AppleVzRunnerCommand.run(
      arguments: ["--allow-real-vz-start"],
      dependencies: fake.dependencies()
    )

    XCTAssertEqual(exitCode, 1)
    XCTAssertEqual(fake.launchVirtualMachineCallCount, 1)
    XCTAssertEqual(
      fake.errorLines,
      [
        "The virtual machine failed to start.; domain=VZErrorDomain; code=1; reason=Internal Virtualization error."
      ]
    )
  }

  func testAllowRealVzStartPassesStopAfterSecondsToLauncher() {
    let launchSpecPath = "/tmp/fast.vmbridge/metadata/apple-vz-launch.json"
    let fake = FakeRunnerDependencies(
      standardInput: readyHandoffJSON(launchSpecPath: launchSpecPath),
      files: [launchSpecPath: readyLinuxKernelLaunchSpecJSON]
    )

    let exitCode = AppleVzRunnerCommand.run(
      arguments: [
        "--allow-real-vz-start",
        "--stop-after-seconds",
        "5",
        "--force-stop-grace-seconds",
        "2",
      ],
      dependencies: fake.dependencies()
    )

    XCTAssertEqual(exitCode, 0)
    XCTAssertEqual(fake.launchVirtualMachineCallCount, 1)
    XCTAssertEqual(fake.launchedSpecs.first?.vmName, "fast-linux")
    XCTAssertEqual(
      fake.launchOptions.first,
      AppleVzLaunchOptions(stopAfterSeconds: 5, forceStopGraceSeconds: 2)
    )
    XCTAssertTrue(fake.errorLines.isEmpty)
  }

  func testStopAfterSecondsAddsDefaultForceStopGrace() {
    let launchSpecPath = "/tmp/fast.vmbridge/metadata/apple-vz-launch.json"
    let fake = FakeRunnerDependencies(
      standardInput: readyHandoffJSON(launchSpecPath: launchSpecPath),
      files: [launchSpecPath: readyLinuxKernelLaunchSpecJSON]
    )

    let exitCode = AppleVzRunnerCommand.run(
      arguments: ["--allow-real-vz-start", "--stop-after-seconds", "5"],
      dependencies: fake.dependencies()
    )

    XCTAssertEqual(exitCode, 0)
    XCTAssertEqual(
      fake.launchOptions.first,
      AppleVzLaunchOptions(stopAfterSeconds: 5, forceStopGraceSeconds: 10)
    )
    XCTAssertTrue(fake.errorLines.isEmpty)
  }

  func testStopAfterSecondsRequiresPositiveInteger() {
    let fake = FakeRunnerDependencies(standardInput: readyHandoffJSON(launchSpecPath: nil))

    let exitCode = AppleVzRunnerCommand.run(
      arguments: ["--stop-after-seconds", "0"],
      dependencies: fake.dependencies()
    )

    XCTAssertEqual(exitCode, 1)
    XCTAssertEqual(fake.launchVirtualMachineCallCount, 0)
    XCTAssertEqual(fake.errorLines, ["--stop-after-seconds requires a positive integer, got 0"])
  }

  func testForceStopGraceSecondsRequiresPositiveInteger() {
    let fake = FakeRunnerDependencies(standardInput: readyHandoffJSON(launchSpecPath: nil))

    let exitCode = AppleVzRunnerCommand.run(
      arguments: ["--force-stop-grace-seconds", "0"],
      dependencies: fake.dependencies()
    )

    XCTAssertEqual(exitCode, 1)
    XCTAssertEqual(fake.launchVirtualMachineCallCount, 0)
    XCTAssertEqual(fake.errorLines, ["--force-stop-grace-seconds requires a positive integer, got 0"])
  }
}

private final class FakeRunnerDependencies {
  private let standardInput: String
  private let files: [String: String]
  private let launchError: Error?

  private(set) var readStandardInputCallCount = 0
  private(set) var readFilePaths: [String] = []
  private(set) var validateVzConfigurationCallCount = 0
  private(set) var launchVirtualMachineCallCount = 0
  private(set) var validatedSpecs: [AppleVzLaunchSpec] = []
  private(set) var launchedSpecs: [AppleVzLaunchSpec] = []
  private(set) var launchOptions: [AppleVzLaunchOptions] = []
  private(set) var outputLines: [String] = []
  private(set) var errorLines: [String] = []

  init(standardInput: String, files: [String: String] = [:], launchError: Error? = nil) {
    self.standardInput = standardInput
    self.files = files
    self.launchError = launchError
  }

  func dependencies() -> AppleVzRunnerCommand.Dependencies {
    AppleVzRunnerCommand.Dependencies(
      readStandardInput: {
        self.readStandardInputCallCount += 1
        return Data(self.standardInput.utf8)
      },
      readFile: { path in
        self.readFilePaths.append(path)
        guard let file = self.files[path] else {
          throw CocoaError(.fileNoSuchFile)
        }
        return Data(file.utf8)
      },
      validateVzConfiguration: { spec in
        self.validateVzConfigurationCallCount += 1
        self.validatedSpecs.append(spec)
      },
      launchVirtualMachine: { spec, options in
        self.launchVirtualMachineCallCount += 1
        self.launchedSpecs.append(spec)
        self.launchOptions.append(options)
        if let launchError = self.launchError {
          throw launchError
        }
      },
      writeOutput: { line in
        self.outputLines.append(line)
      },
      writeError: { line in
        self.errorLines.append(line)
      }
    )
  }
}

private func readyHandoffJSON(launchSpecPath: String?) -> String {
  let launchSpecField = launchSpecPath.map { #""launch_spec_path": "\#($0)","# } ?? ""
  return """
  {
    "backend": "apple-virtualization-framework",
    "vm_name": "fast-linux",
    "bundle_path": "/tmp/fast.vmbridge",
    \(launchSpecField)
    "guest": {
      "os": "ubuntu",
      "arch": "arm64"
    },
    "boot_mode": "linux-kernel",
    "disk": {
      "path": "/tmp/fast.vmbridge/disks/root.raw",
      "format": "raw",
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

private let readyLinuxKernelLaunchSpecJSON = """
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
