import Foundation

#if canImport(Virtualization)
import Virtualization

public enum AppleVzConfigurationBuilder {
  public static func buildLinuxKernelConfiguration(
    spec: AppleVzLaunchSpec
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

    return configuration
  }

  public static func validateLinuxKernelConfiguration(spec: AppleVzLaunchSpec) throws {
    let configuration = try buildLinuxKernelConfiguration(spec: spec)
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
