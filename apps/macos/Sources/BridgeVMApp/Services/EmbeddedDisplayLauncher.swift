import Foundation

/// Launches a Fast Mode (Apple VZ) VM in an embedded display window by spawning
/// the bundled `lightvm-runner` with `--apple-vz-display`, which forwards
/// `--display` to the bundled, signed `AppleVzRunner` (it hosts the VM in a
/// `VZVirtualMachineView` window). The window runs in its own process, so the
/// app does not block.
///
/// The embedded display is intentionally NOT routed through the daemon: the
/// window must live on the user's GUI session, and the display configuration
/// has no suspend/resume. When daemon/app store metadata is available, the
/// runner is launched with that store and the display IPC artifacts are derived
/// from the same VM bundle; otherwise the default store fallback is preserved.
enum EmbeddedDisplayLauncher {
  private static let activeProcessLock = NSLock()
  nonisolated(unsafe) private static var activeProcesses: [String: Process] = [:]

  struct DisplaySize: Equatable {
    var width: Int
    var height: Int

    static let defaultWindow = DisplaySize(width: 1280, height: 800)
  }

  struct StoreMetadata: Equatable {
    var storeRoot: String?
    var bundlePath: String?

    init(storeRoot: String? = nil, bundlePath: String? = nil) {
      self.storeRoot = storeRoot.flatMap(EmbeddedDisplayLauncher.nonEmptyPath)
      self.bundlePath = bundlePath.flatMap(EmbeddedDisplayLauncher.nonEmptyPath)
    }
  }

  enum LaunchError: Error, LocalizedError, Equatable {
    case helperMissing(String)
    case spawnFailed(String)

    var errorDescription: String? {
      switch self {
      case .helperMissing(let name):
        return "The bundled helper '\(name)' is missing from the app bundle."
      case .spawnFailed(let message):
        return "Could not open the display window: \(message)"
      }
    }
  }

  /// Arguments passed to `lightvm-runner` to boot `vmName` with an embedded
  /// display. Pure + testable: builds the VM-name launch form and includes
  /// `--store` only when the app has concrete store metadata.
  static func runnerArguments(
    vmName: String,
    appleVzRunnerPath: String,
    displaySize: DisplaySize = .defaultWindow,
    storePath: String? = nil,
    runtimeControlSocketPath: String? = nil,
    proxyFramebufferRGBAPath: String? = nil,
    proxyFramebufferCaptureIntervalMillis: UInt64? = nil
  ) -> [String] {
    var arguments = [vmName]
    if let storePath = storePath.flatMap(nonEmptyPath) {
      arguments.append(contentsOf: ["--store", storePath])
    }
    arguments.append(contentsOf: [
      "--launch",
      "--require-ready",
      "--apple-vz-runner",
      appleVzRunnerPath,
      "--apple-vz-allow-real-start",
      "--apple-vz-display",
    ])
    arguments.append(contentsOf: [
      "--apple-vz-display-width",
      "\(displaySize.width)",
      "--apple-vz-display-height",
      "\(displaySize.height)",
    ])
    if let runtimeControlSocketPath {
      arguments.append(contentsOf: [
        "--apple-vz-runtime-control-socket",
        runtimeControlSocketPath,
      ])
    }
    if let proxyFramebufferRGBAPath {
      arguments.append(contentsOf: [
        "--apple-vz-proxy-framebuffer-rgba-file",
        proxyFramebufferRGBAPath,
      ])
    }
    if let proxyFramebufferCaptureIntervalMillis {
      arguments.append(contentsOf: [
        "--apple-vz-proxy-framebuffer-capture-interval-ms",
        "\(proxyFramebufferCaptureIntervalMillis)",
      ])
    }
    return arguments
  }

  static func defaultRuntimeControlSocketPath(
    vmName: String,
    environment: [String: String] = ProcessInfo.processInfo.environment
  ) -> String {
    runtimeControlSocketPath(vmName: vmName, storeMetadata: nil, environment: environment)
  }

  static func runtimeControlSocketPath(
    vmName: String,
    storeMetadata: StoreMetadata?,
    environment: [String: String] = ProcessInfo.processInfo.environment
  ) -> String {
    let bundlePath = effectiveBundlePath(
      vmName: vmName,
      storeMetadata: storeMetadata,
      environment: environment
    )
    return "/tmp/bvm-vz-\(stableRuntimeControlSocketHash(bundlePath)).sock"
  }

  static func defaultProxyFramebufferRGBAPath(
    vmName: String,
    environment: [String: String] = ProcessInfo.processInfo.environment
  ) -> String {
    proxyFramebufferRGBAPath(vmName: vmName, storeMetadata: nil, environment: environment)
  }

  static func proxyFramebufferRGBAPath(
    vmName: String,
    storeMetadata: StoreMetadata?,
    environment: [String: String] = ProcessInfo.processInfo.environment
  ) -> String {
    let bundlePath = effectiveBundlePath(
      vmName: vmName,
      storeMetadata: storeMetadata,
      environment: environment
    )
    return appendingPathComponents(
      to: bundlePath,
      components: ["metadata", "apple-vz-display-framebuffer.rgba"]
    )
  }

  static func effectiveStorePath(storeMetadata: StoreMetadata?) -> String? {
    guard let storeMetadata else {
      return nil
    }
    if let storeRoot = storeMetadata.storeRoot {
      return storeRoot
    }
    return storeMetadata.bundlePath.flatMap(storeRootForBundlePath)
  }

  static func effectiveBundlePath(
    vmName: String,
    storeMetadata: StoreMetadata?,
    environment: [String: String] = ProcessInfo.processInfo.environment
  ) -> String {
    if let bundlePath = storeMetadata?.bundlePath {
      return bundlePath
    }
    if let storeRoot = effectiveStorePath(storeMetadata: storeMetadata) {
      return appendingPathComponents(
        to: storeRoot,
        components: ["vms", "\(storeSlug(vmName)).vmbridge"]
      )
    }
    let home = environment["BRIDGEVM_HOME"].flatMap { nonEmptyPath($0) }
      ?? environment["HOME"].flatMap { nonEmptyPath($0).map { "\($0)/.bridgevm" } }
      ?? ".bridgevm"
    return appendingPathComponents(
      to: home,
      components: ["vms", "\(storeSlug(vmName)).vmbridge"]
    )
  }

  /// Resolve the helpers, build the args, and spawn the runner detached. The
  /// returned `Process` is already running.
  @discardableResult
  static func launch(
    vmName: String,
    displaySize: DisplaySize = .defaultWindow,
    storeMetadata: StoreMetadata? = nil,
    helperResolver: (String) -> URL? = { name in
      BundledDaemonSupervisor.bundledHelperURL(named: name)
    },
    spawn: (URL, [String]) throws -> Process = EmbeddedDisplayLauncher.runDetached
  ) throws -> Process {
    guard let lightvmRunner = helperResolver("lightvm-runner") else {
      throw LaunchError.helperMissing("lightvm-runner")
    }
    guard let appleVzRunner = helperResolver("AppleVzRunner") else {
      throw LaunchError.helperMissing("AppleVzRunner")
    }
    let runtimeSocketPath = runtimeControlSocketPath(
      vmName: vmName,
      storeMetadata: storeMetadata
    )
    let args = runnerArguments(
      vmName: vmName,
      appleVzRunnerPath: appleVzRunner.path,
      displaySize: displaySize,
      storePath: effectiveStorePath(storeMetadata: storeMetadata),
      runtimeControlSocketPath: runtimeSocketPath,
      proxyFramebufferRGBAPath: proxyFramebufferRGBAPath(
        vmName: vmName,
        storeMetadata: storeMetadata
      )
    )
    activeProcessLock.lock()
    defer { activeProcessLock.unlock() }
    if let active = activeProcesses[runtimeSocketPath], active.isRunning {
      return active
    }
    activeProcesses.removeValue(forKey: runtimeSocketPath)
    do {
      let process = try spawn(lightvmRunner, args)
      guard process.isRunning else { return process }
      activeProcesses[runtimeSocketPath] = process
      let previousTerminationHandler = process.terminationHandler
      process.terminationHandler = { terminated in
        previousTerminationHandler?(terminated)
        activeProcessLock.lock()
        defer { activeProcessLock.unlock() }
        if activeProcesses[runtimeSocketPath] === terminated {
          activeProcesses.removeValue(forKey: runtimeSocketPath)
        }
      }
      return process
    } catch {
      throw LaunchError.spawnFailed(error.localizedDescription)
    }
  }

  static func runDetached(executableURL: URL, arguments: [String]) throws -> Process {
    let process = Process()
    process.executableURL = executableURL
    process.arguments = arguments
    var environment = ProcessInfo.processInfo.environment
    environment["BRIDGEVM_APPLE_VZ_ALLOW_REAL_START"] = "1"
    process.environment = environment
    try process.run()
    Thread.sleep(forTimeInterval: 0.1)
    guard process.isRunning else {
      process.waitUntilExit()
      let reason = process.terminationReason == .uncaughtSignal
        ? "signal \(process.terminationStatus)"
        : "status \(process.terminationStatus)"
      throw NSError(
        domain: "BridgeVM.EmbeddedDisplayLauncher",
        code: Int(process.terminationStatus),
        userInfo: [NSLocalizedDescriptionKey: "The display helper exited immediately (\(reason))."]
      )
    }
    return process
  }

  private static func nonEmptyPath(_ path: String) -> String? {
    let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? nil : trimmed
  }

  private static func storeRootForBundlePath(_ bundlePath: String) -> String? {
    let bundle = bundlePath as NSString
    guard bundle.pathExtension == "vmbridge" else {
      return nil
    }
    let vmsDirectory = bundle.deletingLastPathComponent as NSString
    guard vmsDirectory.lastPathComponent == "vms" else {
      return nil
    }
    return nonEmptyPath(vmsDirectory.deletingLastPathComponent)
  }

  private static func appendingPathComponents(to base: String, components: [String]) -> String {
    components.reduce(base) { partial, component in
      (partial as NSString).appendingPathComponent(component)
    }
  }

  private static func storeSlug(_ value: String) -> String {
    value
      .map { character in
        character.isASCII && (character.isLetter || character.isNumber)
          ? String(character).lowercased()
          : "-"
      }
      .joined()
      .split(separator: "-")
      .joined(separator: "-")
  }

  private static func stableRuntimeControlSocketHash(_ value: String) -> String {
    var hash: UInt64 = 0xcbf2_9ce4_8422_2325
    for byte in value.utf8 {
      hash ^= UInt64(byte)
      hash = hash &* 0x0000_0100_0000_01b3
    }
    return String(format: "%016llx", hash)
  }
}
