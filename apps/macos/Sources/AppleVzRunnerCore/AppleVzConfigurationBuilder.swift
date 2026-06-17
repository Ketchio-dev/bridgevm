import Foundation

/// A host directory shared into the guest over VZ-native Virtio FS (no
/// `virtiofsd`; the guest mounts it with `mount -t virtiofs <tag> <dir>`).
///
/// Defined outside the `Virtualization` guard so `AppleVzLaunchOptions` can carry
/// shares on any build; the actual VZ device wiring stays guarded below.
public struct AppleVzSharedDirectorySpec: Equatable {
  public var path: String
  public var tag: String
  public var readOnly: Bool

  public init(path: String, tag: String, readOnly: Bool = false) {
    self.path = path
    self.tag = tag
    self.readOnly = readOnly
  }
}

#if canImport(Virtualization)
import Virtualization

public enum AppleVzConfigurationBuilder {
  public static func buildLinuxKernelConfiguration(
    spec: AppleVzLaunchSpec,
    sharedDirectory: AppleVzSharedDirectorySpec? = nil
  ) throws -> VZVirtualMachineConfiguration {
    try buildLinuxKernelConfiguration(
      spec: spec,
      sharedDirectories: sharedDirectory.map { [$0] } ?? []
    )
  }

  public static func buildLinuxKernelConfiguration(
    spec: AppleVzLaunchSpec,
    sharedDirectories: [AppleVzSharedDirectorySpec]
  ) throws -> VZVirtualMachineConfiguration {
    guard ["arm64", "aarch64"].contains(spec.guest.arch.lowercased()) else {
      throw AppleVzRunnerError.unsupportedGuestArch(spec.guest.arch)
    }
    guard spec.boot.mode == "linux-kernel" else {
      throw AppleVzRunnerError.unsupportedBootMode(spec.boot.mode)
    }
    guard spec.disk.format == "raw" else {
      throw AppleVzRunnerError.unsupportedDiskFormat(spec.disk.format)
    }
    guard spec.devices.network == "nat" else {
      throw AppleVzRunnerError.unsupportedNetwork(spec.devices.network)
    }
    guard spec.readiness.ready else {
      throw AppleVzRunnerError.notReady(spec.readiness.blockers)
    }
    guard let kernel = spec.boot.kernel else {
      throw AppleVzRunnerError.missingKernel
    }

    let configuration = VZVirtualMachineConfiguration()
    let platform = VZGenericPlatformConfiguration()
    // Persist a stable machine identifier per VM bundle. Apple VZ save/restore
    // (suspend/resume) rejects a restore whose configuration identity differs
    // from the saved state, so a fresh random identifier on every build would
    // make restore fail with VZErrorDomain "invalid argument".
    platform.machineIdentifier = loadOrCreateMachineIdentifier(bundlePath: spec.bundlePath)
    configuration.platform = platform

    let bootLoader = VZLinuxBootLoader(kernelURL: URL(fileURLWithPath: kernel.path))
    if let initrd = spec.boot.initrd {
      bootLoader.initialRamdiskURL = URL(fileURLWithPath: initrd.path)
    }
    if let commandLine = spec.boot.kernelCommandLine {
      bootLoader.commandLine = commandLine
    }
    configuration.bootLoader = bootLoader

    configuration.cpuCount = try parsePositiveInt(spec.resources.cpu)
    configuration.memorySize = try parsePositiveUInt64(spec.resources.memory) * 1024 * 1024

    let diskAttachment = try VZDiskImageStorageDeviceAttachment(
      url: URL(fileURLWithPath: spec.disk.path),
      readOnly: spec.disk.readOnly
    )
    configuration.storageDevices = [
      VZVirtioBlockDeviceConfiguration(attachment: diskAttachment)
    ]

    let networkDevice = VZVirtioNetworkDeviceConfiguration()
    // Persist a stable MAC per VM bundle. Like the machine identifier, an
    // unstable (randomly generated) MAC makes the save-time and restore-time
    // configurations differ, so VZ save/restore (suspend/resume) rejects the
    // restore with VZErrorRestore "invalid argument".
    networkDevice.macAddress = loadOrCreateMACAddress(bundlePath: spec.bundlePath)
    networkDevice.attachment = VZNATNetworkDeviceAttachment()
    configuration.networkDevices = [networkDevice]

    if !spec.devices.serialLogPath.isEmpty {
      configuration.serialPorts = [
        try buildSerialPort(logPath: spec.devices.serialLogPath)
      ]
    }

    if spec.devices.entropyDevice {
      configuration.entropyDevices = [VZVirtioEntropyDeviceConfiguration()]
    }
    if spec.resources.balloonDevice {
      configuration.memoryBalloonDevices = [
        VZVirtioTraditionalMemoryBalloonDeviceConfiguration()
      ]
    }

    if !sharedDirectories.isEmpty {
      // VZ-native Virtio FS shared folder(s) (macOS 13+). The guest mounts each
      // with `mount -t virtiofs <tag> <dir>`. No virtiofsd needed (that is
      // QEMU-only and unavailable on macOS). A single share uses
      // VZSingleDirectoryShare; 2+ shares are attached on one Virtio-FS device as
      // a VZMultipleDirectoryShare keyed by tag.
      guard #available(macOS 13.0, *) else {
        throw AppleVzRunnerError.sharedDirectoryRequiresMacOS13
      }
      // VZ requires every share tag to be unique within the device; reject
      // duplicates loudly rather than silently dropping a colliding folder.
      var seenTags = Set<String>()
      for share in sharedDirectories {
        try VZVirtioFileSystemDeviceConfiguration.validateTag(share.tag)
        guard seenTags.insert(share.tag).inserted else {
          throw AppleVzConfigurationBuilderError.duplicateSharedDirectoryTag(share.tag)
        }
      }

      let device: VZVirtioFileSystemDeviceConfiguration
      if sharedDirectories.count == 1, let share = sharedDirectories.first {
        device = VZVirtioFileSystemDeviceConfiguration(tag: share.tag)
        device.share = VZSingleDirectoryShare(
          directory: VZSharedDirectory(
            url: URL(fileURLWithPath: share.path),
            readOnly: share.readOnly
          )
        )
      } else {
        // VZMultipleDirectoryShare carries all folders on one device, keyed by
        // tag. The device itself needs a (validated, unique) tag too; reuse the
        // first share's tag, which is already in the unique set.
        var directories: [String: VZSharedDirectory] = [:]
        for share in sharedDirectories {
          directories[share.tag] = VZSharedDirectory(
            url: URL(fileURLWithPath: share.path),
            readOnly: share.readOnly
          )
        }
        device = VZVirtioFileSystemDeviceConfiguration(tag: sharedDirectories[0].tag)
        device.share = VZMultipleDirectoryShare(directories: directories)
      }
      configuration.directorySharingDevices = [device]
    }

    return configuration
  }

  public static func validateLinuxKernelConfiguration(spec: AppleVzLaunchSpec) throws {
    let configuration = try buildLinuxKernelConfiguration(spec: spec)
    try configuration.validate()
  }

  /// Build the headless Linux configuration and add the devices needed to show
  /// an embedded graphical display: a Virtio GPU scanout the guest renders to,
  /// plus a USB keyboard and pointing device so the `VZVirtualMachineView` can
  /// forward input. Kept entirely separate from `buildLinuxKernelConfiguration`
  /// so the verified headless boot + save/restore path is untouched. Note: a VZ
  /// VM with a graphics device generally cannot be saved/restored, so the
  /// windowed display path does not offer suspend/resume.
  @available(macOS 14.0, *)
  public static func buildLinuxKernelConfigurationWithDisplay(
    spec: AppleVzLaunchSpec,
    widthInPixels: Int = 1280,
    heightInPixels: Int = 800,
    sharedDirectory: AppleVzSharedDirectorySpec? = nil
  ) throws -> VZVirtualMachineConfiguration {
    try buildLinuxKernelConfigurationWithDisplay(
      spec: spec,
      widthInPixels: widthInPixels,
      heightInPixels: heightInPixels,
      sharedDirectories: sharedDirectory.map { [$0] } ?? []
    )
  }

  @available(macOS 14.0, *)
  public static func buildLinuxKernelConfigurationWithDisplay(
    spec: AppleVzLaunchSpec,
    widthInPixels: Int = 1280,
    heightInPixels: Int = 800,
    sharedDirectories: [AppleVzSharedDirectorySpec]
  ) throws -> VZVirtualMachineConfiguration {
    let configuration = try buildLinuxKernelConfiguration(
      spec: spec, sharedDirectories: sharedDirectories)

    let graphics = VZVirtioGraphicsDeviceConfiguration()
    graphics.scanouts = [
      VZVirtioGraphicsScanoutConfiguration(
        widthInPixels: widthInPixels,
        heightInPixels: heightInPixels
      )
    ]
    configuration.graphicsDevices = [graphics]
    configuration.keyboards = [VZUSBKeyboardConfiguration()]
    configuration.pointingDevices = [VZUSBScreenCoordinatePointingDeviceConfiguration()]

    return configuration
  }

  @available(macOS 14.0, *)
  public static func validateLinuxKernelConfigurationWithDisplay(spec: AppleVzLaunchSpec) throws {
    let configuration = try buildLinuxKernelConfigurationWithDisplay(spec: spec)
    try configuration.validate()
  }

  private static func loadOrCreateMachineIdentifier(
    bundlePath: String
  ) -> VZGenericMachineIdentifier {
    guard !bundlePath.isEmpty else {
      return VZGenericMachineIdentifier()
    }
    let metadataDir = URL(fileURLWithPath: bundlePath).appendingPathComponent("metadata")
    let identifierFile = metadataDir.appendingPathComponent("machine-identifier.bin")
    if let data = try? Data(contentsOf: identifierFile),
      let identifier = VZGenericMachineIdentifier(dataRepresentation: data)
    {
      return identifier
    }
    let identifier = VZGenericMachineIdentifier()
    persistStableIdentity(
      identifier.dataRepresentation, to: identifierFile, label: "machine identifier")
    return identifier
  }

  private static func loadOrCreateMACAddress(bundlePath: String) -> VZMACAddress {
    guard !bundlePath.isEmpty else {
      return VZMACAddress.randomLocallyAdministered()
    }
    let macFile = URL(fileURLWithPath: bundlePath)
      .appendingPathComponent("metadata")
      .appendingPathComponent("network-mac-address.txt")
    if let stored = try? String(contentsOf: macFile, encoding: .utf8)
      .trimmingCharacters(in: .whitespacesAndNewlines),
      let mac = VZMACAddress(string: stored)
    {
      return mac
    }
    let mac = VZMACAddress.randomLocallyAdministered()
    persistStableIdentity(Data(mac.string.utf8), to: macFile, label: "network MAC address")
    return mac
  }

  /// Persist a stable-identity artifact (machine identifier, MAC) atomically.
  /// These must survive across processes for Apple VZ save/restore to match the
  /// saved state, so a write failure is a loud warning, not silent.
  private static func persistStableIdentity(_ data: Data, to file: URL, label: String) {
    do {
      try FileManager.default.createDirectory(
        at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
      try data.write(to: file, options: .atomic)
    } catch {
      FileHandle.standardError.write(
        Data(
          "AppleVzRunner: WARNING failed to persist \(label) at \(file.path): \(error). Suspend/resume restore will fail because the VM configuration identity will not be stable.\n"
            .utf8))
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

  private static func buildSerialPort(logPath: String) throws -> VZSerialPortConfiguration {
    let logURL = URL(fileURLWithPath: logPath)
    try FileManager.default.createDirectory(
      at: logURL.deletingLastPathComponent(),
      withIntermediateDirectories: true
    )
    if !FileManager.default.fileExists(atPath: logURL.path) {
      FileManager.default.createFile(atPath: logURL.path, contents: nil)
    }
    let output = try FileHandle(forWritingTo: logURL)
    try output.seekToEnd()

    let serialPort = VZVirtioConsoleDeviceSerialPortConfiguration()
    serialPort.attachment = VZFileHandleSerialPortAttachment(
      fileHandleForReading: nil,
      fileHandleForWriting: output
    )
    return serialPort
  }
}
#endif

/// Errors raised while assembling the VZ configuration that are specific to the
/// builder (kept here, rather than in the shared `AppleVzRunnerError`, so the
/// multi-share wiring owns its own failure mode).
public enum AppleVzConfigurationBuilderError: Error, LocalizedError, Equatable {
  /// Two shared folders resolved to the same Virtio-FS tag. VZ requires every
  /// tag to be unique, so the colliding share is rejected rather than dropped.
  case duplicateSharedDirectoryTag(String)

  public var errorDescription: String? {
    switch self {
    case .duplicateSharedDirectoryTag(let tag):
      return "AppleVzRunner shared folders require unique tags; tag '\(tag)' is used more than once"
    }
  }
}
