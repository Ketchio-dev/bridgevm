import Foundation

/// Launches a Fast Mode (Apple VZ) VM in an embedded display window by spawning
/// the bundled `lightvm-runner` with `--apple-vz-display`, which forwards
/// `--display` to the bundled, signed `AppleVzRunner` (it hosts the VM in a
/// `VZVirtualMachineView` window). The window runs in its own process, so the
/// app does not block.
///
/// The embedded display is intentionally NOT routed through the daemon: the
/// window must live on the user's GUI session, and the display configuration
/// has no suspend/resume. The runner reads the same default store the bundled
/// daemon uses, so no `--store` override is needed.
enum EmbeddedDisplayLauncher {
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
  /// display. Pure + testable: builds the VM-name launch form (no `--store`, so
  /// the runner uses the same default store as the bundled daemon).
  static func runnerArguments(vmName: String, appleVzRunnerPath: String) -> [String] {
    [
      vmName,
      "--launch",
      "--require-ready",
      "--apple-vz-runner",
      appleVzRunnerPath,
      "--apple-vz-allow-real-start",
      "--apple-vz-display",
    ]
  }

  /// Resolve the helpers, build the args, and spawn the runner detached. The
  /// returned `Process` is already running.
  @discardableResult
  static func launch(
    vmName: String,
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
    let args = runnerArguments(vmName: vmName, appleVzRunnerPath: appleVzRunner.path)
    do {
      return try spawn(lightvmRunner, args)
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
    return process
  }
}
