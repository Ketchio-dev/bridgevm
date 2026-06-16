import Foundation

#if canImport(Virtualization)
import Virtualization
#endif

public struct AppleVzLaunchHandoff: Codable, Equatable {
  public var backend: String
  public var vmName: String
  public var bundlePath: String
  public var launchSpecPath: String?
  public var guest: AppleVzGuestSpec
  public var bootMode: String
  public var disk: AppleVzDiskSpec
  public var resources: AppleVzResourceSpec
  public var runnerLogPath: String
  public var serialLogPath: String
  public var integration: AppleVzIntegrationSpec
  public var readiness: AppleVzReadinessSpec

  public init(
    backend: String,
    vmName: String,
    bundlePath: String,
    launchSpecPath: String?,
    guest: AppleVzGuestSpec,
    bootMode: String,
    disk: AppleVzDiskSpec,
    resources: AppleVzResourceSpec,
    runnerLogPath: String,
    serialLogPath: String,
    integration: AppleVzIntegrationSpec,
    readiness: AppleVzReadinessSpec
  ) {
    self.backend = backend
    self.vmName = vmName
    self.bundlePath = bundlePath
    self.launchSpecPath = launchSpecPath
    self.guest = guest
    self.bootMode = bootMode
    self.disk = disk
    self.resources = resources
    self.runnerLogPath = runnerLogPath
    self.serialLogPath = serialLogPath
    self.integration = integration
    self.readiness = readiness
  }

  enum CodingKeys: String, CodingKey {
    case backend
    case vmName = "vm_name"
    case bundlePath = "bundle_path"
    case launchSpecPath = "launch_spec_path"
    case guest
    case bootMode = "boot_mode"
    case disk
    case resources
    case runnerLogPath = "runner_log_path"
    case serialLogPath = "serial_log_path"
    case integration
    case readiness
  }
}

public struct AppleVzLaunchSpec: Codable, Equatable {
  public var vmName: String
  public var bundlePath: String
  public var guest: AppleVzGuestSpec
  public var boot: AppleVzBootSpec
  public var disk: AppleVzDiskSpec
  public var resources: AppleVzResourceSpec
  public var devices: AppleVzDeviceSpec
  public var integration: AppleVzIntegrationSpec
  public var logs: AppleVzLogSpec
  public var readiness: AppleVzReadinessSpec

  enum CodingKeys: String, CodingKey {
    case vmName = "vm_name"
    case bundlePath = "bundle_path"
    case guest
    case boot
    case disk
    case resources
    case devices
    case integration
    case logs
    case readiness
  }
}

public struct AppleVzGuestSpec: Codable, Equatable {
  public var os: String
  public var arch: String
}

public struct AppleVzBootSpec: Codable, Equatable {
  public var mode: String
  public var installerImage: AppleVzPathSpec?
  public var kernel: AppleVzPathSpec?
  public var initrd: AppleVzPathSpec?
  public var kernelCommandLine: String?
  public var macosRestoreImage: AppleVzPathSpec?

  enum CodingKeys: String, CodingKey {
    case mode
    case installerImage = "installer_image"
    case kernel
    case initrd
    case kernelCommandLine = "kernel_command_line"
    case macosRestoreImage = "macos_restore_image"
  }
}

public struct AppleVzPathSpec: Codable, Equatable {
  public var path: String
  public var exists: Bool
}

public struct AppleVzDiskSpec: Codable, Equatable {
  public var path: String
  public var format: String
  public var readOnly: Bool

  enum CodingKeys: String, CodingKey {
    case path
    case format
    case readOnly = "read_only"
  }
}

public struct AppleVzResourceSpec: Codable, Equatable {
  public var memory: String
  public var cpu: String
  public var displayFPSCap: String
  public var rationale: String
  public var balloonDevice: Bool

  enum CodingKeys: String, CodingKey {
    case memory
    case cpu
    case displayFPSCap = "display_fps_cap"
    case rationale
    case balloonDevice = "balloon_device"
  }
}

public struct AppleVzIntegrationSpec: Codable, Equatable {
  public var clipboard: Bool
  public var dynamicResolution: Bool
  public var sharedFolders: Bool
  public var virtiofs: Bool

  enum CodingKeys: String, CodingKey {
    case clipboard
    case dynamicResolution = "dynamic_resolution"
    case sharedFolders = "shared_folders"
    case virtiofs
  }
}

public struct AppleVzDeviceSpec: Codable, Equatable {
  public var entropyDevice: Bool
  public var network: String
  public var serialLogPath: String

  enum CodingKeys: String, CodingKey {
    case entropyDevice = "entropy_device"
    case network
    case serialLogPath = "serial_log_path"
  }
}

public struct AppleVzLogSpec: Codable, Equatable {
  public var runnerLogPath: String

  enum CodingKeys: String, CodingKey {
    case runnerLogPath = "runner_log_path"
  }
}

public struct AppleVzReadinessSpec: Codable, Equatable {
  public var ready: Bool
  public var blockers: [AppleVzReadinessBlocker]
}

public struct AppleVzReadinessBlocker: Codable, Equatable {
  public var code: String
  public var message: String
  public var path: String?
  public var capability: String?
}

public enum AppleVzRunnerError: Error, LocalizedError, Equatable {
  case unsupportedBackend(String)
  case unsupportedGuestArch(String)
  case unsupportedBootMode(String)
  case unsupportedDiskFormat(String)
  case unsupportedNetwork(String)
  case missingKernel
  case invalidMemory(String)
  case invalidCPU(String)
  case notReady([AppleVzReadinessBlocker])

  public var errorDescription: String? {
    switch self {
    case .unsupportedBackend(let backend):
      return "AppleVzRunner requires backend apple-virtualization-framework, got \(backend)"
    case .unsupportedGuestArch(let arch):
      return "AppleVzRunner requires guest arch arm64/aarch64, got \(arch)"
    case .unsupportedBootMode(let mode):
      return "AppleVzRunner does not support boot mode \(mode) yet"
    case .unsupportedDiskFormat(let format):
      return "AppleVzRunner requires disk format raw/qcow2, got \(format)"
    case .unsupportedNetwork(let network):
      return "AppleVzRunner requires nat networking, got \(network)"
    case .missingKernel:
      return "AppleVzRunner linux-kernel configuration requires a kernel path"
    case .invalidMemory(let memory):
      return "AppleVzRunner requires numeric memory MiB, got \(memory)"
    case .invalidCPU(let cpu):
      return "AppleVzRunner requires numeric CPU count, got \(cpu)"
    case .notReady(let blockers):
      return "AppleVzRunner handoff is not launch-ready: \(Self.blockerSummary(blockers))"
    }
  }

  private static func blockerSummary(_ blockers: [AppleVzReadinessBlocker]) -> String {
    if blockers.isEmpty {
      return "unknown blocker"
    }
    return blockers.map { blocker in
      if let path = blocker.path {
        return "\(blocker.code): \(blocker.message) (\(path))"
      }
      if let capability = blocker.capability {
        return "\(blocker.code): \(blocker.message) (\(capability))"
      }
      return "\(blocker.code): \(blocker.message)"
    }.joined(separator: "; ")
  }
}

public struct AppleVzRunnerValidation: Equatable {
  public var vmName: String
  public var backend: String
  public var bootMode: String
  public var diskPath: String
  public var memoryMiB: UInt64
  public var cpuCount: Int
  public var virtualizationFrameworkLinked: Bool
}

public struct AppleVzConfigurationPlan: Equatable {
  public var vmName: String
  public var bootMode: String
  public var bootLoader: String
  public var platform: String
  public var diskAttachment: String
  public var networkAttachment: String
  public var memoryBytes: UInt64
  public var cpuCount: Int
  public var entropyDevice: Bool
  public var balloonDevice: Bool
  public var serialLogPath: String
}

public enum AppleVzHandoffValidator {
  public static func decode(_ data: Data) throws -> AppleVzLaunchHandoff {
    try JSONDecoder().decode(AppleVzLaunchHandoff.self, from: data)
  }

  public static func validate(_ handoff: AppleVzLaunchHandoff) throws -> AppleVzRunnerValidation {
    guard handoff.backend == "apple-virtualization-framework" else {
      throw AppleVzRunnerError.unsupportedBackend(handoff.backend)
    }
    guard ["arm64", "aarch64"].contains(handoff.guest.arch.lowercased()) else {
      throw AppleVzRunnerError.unsupportedGuestArch(handoff.guest.arch)
    }
    guard ["existing-disk", "linux-installer", "linux-kernel", "macos-restore"].contains(handoff.bootMode) else {
      throw AppleVzRunnerError.unsupportedBootMode(handoff.bootMode)
    }
    guard ["raw", "qcow2"].contains(handoff.disk.format) else {
      throw AppleVzRunnerError.unsupportedDiskFormat(handoff.disk.format)
    }
    guard handoff.readiness.ready else {
      throw AppleVzRunnerError.notReady(handoff.readiness.blockers)
    }
    let memoryMiB = try parsePositiveUInt64(handoff.resources.memory)
    let cpuCount = try parsePositiveInt(handoff.resources.cpu)

    return AppleVzRunnerValidation(
      vmName: handoff.vmName,
      backend: handoff.backend,
      bootMode: handoff.bootMode,
      diskPath: handoff.disk.path,
      memoryMiB: memoryMiB,
      cpuCount: cpuCount,
      virtualizationFrameworkLinked: virtualizationFrameworkLinked()
    )
  }

  public static func configurationPlan(for handoff: AppleVzLaunchHandoff) throws -> AppleVzConfigurationPlan {
    let validation = try validate(handoff)
    return AppleVzConfigurationPlan(
      vmName: validation.vmName,
      bootMode: validation.bootMode,
      bootLoader: bootLoaderName(for: handoff.bootMode),
      platform: "generic",
      diskAttachment: handoff.disk.format == "qcow2" ? "disk-image-qcow2" : "disk-image-raw",
      networkAttachment: "nat",
      memoryBytes: validation.memoryMiB * 1024 * 1024,
      cpuCount: validation.cpuCount,
      entropyDevice: true,
      balloonDevice: handoff.resources.balloonDevice,
      serialLogPath: handoff.serialLogPath
    )
  }

  public static func virtualizationFrameworkLinked() -> Bool {
    #if canImport(Virtualization)
    return true
    #else
    return false
    #endif
  }

  private static func bootLoaderName(for bootMode: String) -> String {
    switch bootMode {
    case "linux-kernel":
      return "linux-kernel"
    case "linux-installer", "existing-disk", "macos-restore":
      return "efi"
    default:
      return "unknown"
    }
  }

  private static func parsePositiveUInt64(_ value: String) throws -> UInt64 {
    guard let parsed = UInt64(value), parsed > 0 else {
      throw AppleVzRunnerError.invalidMemory(value)
    }
    return parsed
  }

  private static func parsePositiveInt(_ value: String) throws -> Int {
    guard let parsed = Int(value), parsed > 0 else {
      throw AppleVzRunnerError.invalidCPU(value)
    }
    return parsed
  }
}
