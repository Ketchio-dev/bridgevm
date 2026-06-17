import Darwin
import Foundation

struct BundledDaemonLaunchReport: Equatable {
  enum State: Equatable {
    case disabledByMockInventory
    case customSocket
    case alreadyRunning
    case missingHelper
    case running
    case failed
  }

  var state: State
  var helperPath: String?
  var socketPath: String?
  var detail: String
  var stderrTail: String?

  var isHealthy: Bool {
    switch state {
    case .alreadyRunning, .running, .disabledByMockInventory, .customSocket:
      return true
    case .missingHelper, .failed:
      return false
    }
  }
}

struct BundledDaemonProcess {
  var process: Process
  var stderrTail: () -> String?
  var cleanup: () -> Void

  init(
    process: Process,
    stderrTail: @escaping () -> String? = { nil },
    cleanup: @escaping () -> Void = {}
  ) {
    self.process = process
    self.stderrTail = stderrTail
    self.cleanup = cleanup
  }
}

struct BundledDaemonLaunchError: LocalizedError {
  var message: String
  var stderrTail: String?

  var errorDescription: String? {
    message
  }
}

final class BundledDaemonSupervisor {
  static let shared = BundledDaemonSupervisor()

  private var process: Process?
  private var activeProcessCleanup: (() -> Void)?
  private var activeAllowsAppleVzRealStart = false
  private(set) var lastLaunchReport: BundledDaemonLaunchReport?

  func startIfNeeded(settings: AppSettings) -> BundledDaemonLaunchReport {
    let report = startIfNeeded(
      settings: settings,
      helperResolver: { Self.bundledHelperURL(named: $0) },
      launcher: Self.runDaemonProcess
    )
    lastLaunchReport = report
    return report
  }

  func startIfNeeded(
    settings: AppSettings,
    helperResolver: (String) -> URL?,
    launcher: (URL, [String: String]) throws -> BundledDaemonProcess,
    livenessProbeDelay: TimeInterval = 0.05,
    socketReadinessTimeout: TimeInterval = 2.0,
    socketReadinessPollInterval: TimeInterval = 0.05,
    socketReadyProbe: (String) -> Bool = BundledDaemonSupervisor.isDaemonSocketReady
  ) -> BundledDaemonLaunchReport {
    let socketPath = settings.effectiveDaemonSocketPath
    guard !settings.useMockInventory else {
      _ = stop()
      return BundledDaemonLaunchReport(
        state: .disabledByMockInventory,
        helperPath: nil,
        socketPath: socketPath,
        detail: "Bundled daemon launch skipped because mock inventory is enabled."
      )
    }
    guard socketPath == DaemonEndpoint.local.socketPath else {
      _ = stop()
      return BundledDaemonLaunchReport(
        state: .customSocket,
        helperPath: nil,
        socketPath: socketPath,
        detail: "Bundled daemon launch skipped because a custom daemon socket is configured."
      )
    }
    if process?.isRunning == true && activeAllowsAppleVzRealStart == settings.allowAppleVzRealStart {
      return BundledDaemonLaunchReport(
        state: .alreadyRunning,
        helperPath: process?.executableURL?.path,
        socketPath: socketPath,
        detail: "Bundled bridgevmd is already running."
      )
    }
    if process?.isRunning == true {
      _ = stop()
    }
    guard let bridgevmd = helperResolver("bridgevmd") else {
      return BundledDaemonLaunchReport(
        state: .missingHelper,
        helperPath: nil,
        socketPath: socketPath,
        detail: "Bundled bridgevmd helper is missing or is not executable."
      )
    }

    do {
      let launched = try launcher(
        bridgevmd,
        Self.daemonEnvironment(
          helperResolver: helperResolver,
          allowAppleVzRealStart: settings.allowAppleVzRealStart
        )
      )
      if livenessProbeDelay > 0 {
        Thread.sleep(forTimeInterval: livenessProbeDelay)
      }
      guard launched.process.isRunning else {
        let stderrTail = launched.stderrTail()
        launched.cleanup()
        process = nil
        return BundledDaemonLaunchReport(
          state: .failed,
          helperPath: bridgevmd.path,
          socketPath: socketPath,
          detail: "Bundled bridgevmd exited immediately after launch.",
          stderrTail: stderrTail
        )
      }
      guard Self.waitForDaemonSocket(
        at: socketPath,
        process: launched.process,
        timeout: socketReadinessTimeout,
        pollInterval: socketReadinessPollInterval,
        socketReadyProbe: socketReadyProbe
      ) else {
        let stderrTail = launched.stderrTail()
        let exitedBeforeReady = !launched.process.isRunning
        if launched.process.isRunning {
          Self.terminateProcess(launched.process, timeout: 0.5)
        }
        launched.cleanup()
        process = nil
        let detail: String
        if exitedBeforeReady {
          detail = "Bundled bridgevmd exited before creating a ready socket."
        } else {
          detail = "Bundled bridgevmd did not create a ready socket before timeout."
        }
        return BundledDaemonLaunchReport(
          state: .failed,
          helperPath: bridgevmd.path,
          socketPath: socketPath,
          detail: detail,
          stderrTail: stderrTail
        )
      }
      process = launched.process
      activeProcessCleanup = launched.cleanup
      activeAllowsAppleVzRealStart = settings.allowAppleVzRealStart
      return BundledDaemonLaunchReport(
        state: .running,
        helperPath: bridgevmd.path,
        socketPath: socketPath,
        detail: "Bundled bridgevmd launched from \(bridgevmd.path) and its socket is ready."
      )
    } catch {
      process = nil
      activeProcessCleanup?()
      activeProcessCleanup = nil
      return BundledDaemonLaunchReport(
        state: .failed,
        helperPath: bridgevmd.path,
        socketPath: socketPath,
        detail: "Bundled bridgevmd failed to launch: \(error.localizedDescription)",
        stderrTail: (error as? BundledDaemonLaunchError)?.stderrTail
      )
    }
  }

  @discardableResult
  func stop(timeout: TimeInterval = 2.0) -> Bool {
    guard let process else { return true }
    if process.isRunning {
      process.terminate()
      let deadline = Date().addingTimeInterval(timeout)
      while process.isRunning && Date() < deadline {
        Thread.sleep(forTimeInterval: 0.05)
      }
      if process.isRunning {
        process.interrupt()
        Thread.sleep(forTimeInterval: 0.05)
      }
    }
    self.process = nil
    activeProcessCleanup?()
    activeProcessCleanup = nil
    activeAllowsAppleVzRealStart = false
    return !process.isRunning
  }

  static func runDaemonProcess(
    executableURL: URL,
    environment: [String: String]
  ) throws -> BundledDaemonProcess {
    let outputCapture = try BundledDaemonOutputCapture()
    let launched = Process()
    launched.executableURL = executableURL
    launched.environment = environment
    launched.standardOutput = outputCapture.pipe
    launched.standardError = outputCapture.pipe
    do {
      try launched.run()
      outputCapture.closeWriteEnd()
    } catch {
      let stderrTail = outputCapture.stderrTail()
      outputCapture.cleanup()
      throw BundledDaemonLaunchError(
        message: error.localizedDescription,
        stderrTail: stderrTail
      )
    }
    return BundledDaemonProcess(
      process: launched,
      stderrTail: { outputCapture.stderrTail() },
      cleanup: { outputCapture.cleanup() }
    )
  }

  static func daemonEnvironment(
    base: [String: String] = ProcessInfo.processInfo.environment,
    helperResolver: (String) -> URL? = { name in
      BundledDaemonSupervisor.bundledHelperURL(named: name)
    },
    allowAppleVzRealStart: Bool = false
  ) -> [String: String] {
    var environment = base
    if let lightvmRunner = helperResolver("lightvm-runner") {
      environment["BRIDGEVM_LIGHTVM_RUNNER"] = lightvmRunner.path
    }
    if let appleVzRunner = helperResolver("AppleVzRunner") {
      environment["BRIDGEVM_APPLE_VZ_RUNNER"] = appleVzRunner.path
    }
    if allowAppleVzRealStart {
      environment["BRIDGEVM_APPLE_VZ_ALLOW_REAL_START"] = "1"
    } else {
      environment.removeValue(forKey: "BRIDGEVM_APPLE_VZ_ALLOW_REAL_START")
    }
    return environment
  }

  static func bundledHelperURL(named name: String, bundle: Bundle = .main) -> URL? {
    if let url = bundle.url(forAuxiliaryExecutable: name) {
      return url
    }
    guard let executableURL = bundle.executableURL else {
      return nil
    }
    let candidate = executableURL
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .appendingPathComponent("Helpers")
      .appendingPathComponent(name)
    return FileManager.default.isExecutableFile(atPath: candidate.path) ? candidate : nil
  }

  static func waitForDaemonSocket(
    at socketPath: String,
    process: Process,
    timeout: TimeInterval,
    pollInterval: TimeInterval,
    socketReadyProbe: (String) -> Bool = BundledDaemonSupervisor.isDaemonSocketReady
  ) -> Bool {
    let deadline = Date().addingTimeInterval(max(0, timeout))
    repeat {
      if socketReadyProbe(socketPath) {
        return true
      }
      guard process.isRunning else {
        return false
      }
      if Date() >= deadline {
        return false
      }
      Thread.sleep(forTimeInterval: max(0.001, pollInterval))
    } while true
  }

  static func terminateProcess(_ process: Process, timeout: TimeInterval) {
    guard process.isRunning else {
      return
    }
    process.terminate()
    let deadline = Date().addingTimeInterval(max(0, timeout))
    while process.isRunning && Date() < deadline {
      Thread.sleep(forTimeInterval: 0.05)
    }
    if process.isRunning {
      process.interrupt()
    }
  }

  static func isDaemonSocketReady(_ socketPath: String) -> Bool {
    let fileDescriptor = socket(AF_UNIX, SOCK_STREAM, 0)
    guard fileDescriptor >= 0 else {
      return false
    }
    defer {
      close(fileDescriptor)
    }

    var address = sockaddr_un()
    address.sun_family = sa_family_t(AF_UNIX)
    let maxPathLength = MemoryLayout.size(ofValue: address.sun_path)
    guard socketPath.utf8.count < maxPathLength else {
      return false
    }
    _ = socketPath.withCString { source in
      withUnsafeMutablePointer(to: &address.sun_path) { destination in
        destination.withMemoryRebound(to: CChar.self, capacity: maxPathLength) {
          strncpy($0, source, maxPathLength)
        }
      }
    }

    let result = withUnsafePointer(to: &address) { pointer in
      pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
        connect(
          fileDescriptor,
          sockaddrPointer,
          socklen_t(MemoryLayout<sockaddr_un>.size)
        )
      }
    }
    return result == 0
  }
}

private final class BundledDaemonOutputCapture {
  private static let maxCapturedBytes = 64 * 1024
  private static let maxTailBytes = 8 * 1024

  let pipe = Pipe()

  private let logURL: URL
  private let writer: FileHandle
  private let queue = DispatchQueue(label: "app.bridgevm.bundled-daemon-output-capture")
  private var capturedData = Data()
  private var didCleanup = false

  init() throws {
    logURL = FileManager.default.temporaryDirectory
      .appendingPathComponent("bridgevmd-\(UUID().uuidString).log")
    FileManager.default.createFile(atPath: logURL.path, contents: nil)
    writer = try FileHandle(forWritingTo: logURL)
    pipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
      let data = handle.availableData
      guard !data.isEmpty else { return }
      self?.append(data)
    }
  }

  func stderrTail() -> String? {
    queue.sync {
      writer.synchronizeFile()
      guard let reader = try? FileHandle(forReadingFrom: logURL) else {
        return nil
      }
      defer {
        try? reader.close()
      }
      do {
        let size = try reader.seekToEnd()
        guard size > 0 else { return nil }
        let maxTailBytes = UInt64(Self.maxTailBytes)
        try reader.seek(toOffset: size > maxTailBytes ? size - maxTailBytes : 0)
        let data = reader.readDataToEndOfFile()
        let tail = String(decoding: data, as: UTF8.self)
          .trimmingCharacters(in: .whitespacesAndNewlines)
        return tail.isEmpty ? nil : tail
      } catch {
        return nil
      }
    }
  }

  func cleanup() {
    queue.sync {
      guard !didCleanup else { return }
      didCleanup = true
      pipe.fileHandleForReading.readabilityHandler = nil
      try? pipe.fileHandleForWriting.close()
      try? writer.close()
      try? pipe.fileHandleForReading.close()
      try? FileManager.default.removeItem(at: logURL)
    }
  }

  private func append(_ data: Data) {
    queue.async {
      guard !self.didCleanup else {
        return
      }
      if data.count >= Self.maxCapturedBytes {
        self.capturedData = Data(data.suffix(Self.maxCapturedBytes))
      } else {
        self.capturedData.append(data)
        if self.capturedData.count > Self.maxCapturedBytes {
          self.capturedData.removeFirst(self.capturedData.count - Self.maxCapturedBytes)
        }
      }
      try? self.writer.truncate(atOffset: 0)
      try? self.writer.seek(toOffset: 0)
      self.writer.write(self.capturedData)
    }
  }

  func closeWriteEnd() {
    try? pipe.fileHandleForWriting.close()
  }
}
