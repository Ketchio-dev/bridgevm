import Foundation

#if canImport(Darwin)
import Darwin
#endif

#if canImport(Virtualization)
import Virtualization
#endif

public enum AppleVzVirtualMachineLaunchResult: Equatable {
  case stopped
  case interrupted
}

public struct AppleVzLaunchOptions: Equatable {
  public var stopAfterSeconds: UInt64?
  public var forceStopGraceSeconds: UInt64?
  /// When set, the VM is paused and its full machine state is written to this
  /// path after `stopAfterSeconds`, instead of being stopped (suspend).
  public var saveStatePath: String?
  /// When set, the VM is restored from this saved state and resumed instead of
  /// performing a fresh boot (resume).
  public var restoreStatePath: String?
  /// When true, boot with a graphical display and host the VM in an on-screen
  /// `VZVirtualMachineView` window (the embedded display). Mutually exclusive
  /// with save/restore (a VZ VM with a graphics device cannot be saved).
  public var displayWindow: Bool
  /// Width for the graphical scanout used by `--display` and `--graphics`.
  public var displayWidthInPixels: Int
  /// Height for the graphical scanout used by `--display` and `--graphics`.
  public var displayHeightInPixels: Int
  /// When true, boot the VM with the graphics-device configuration but WITHOUT a
  /// window, on the headless launcher. This is a verification path: it proves a
  /// real guest boots with the Virtio GPU attached (observable on the serial
  /// console) on a host with no window server, which the windowed `displayWindow`
  /// path cannot do.
  public var graphicsHeadlessVerification: Bool
  /// Host directories shared into the guest over VZ-native Virtio FS, each with
  /// its own mount tag. The guest mounts each with `mount -t virtiofs <tag> <dir>`.
  /// VZ requires every tag to be unique; the Rust planner derives unique tags.
  public var sharedDirectorySpecs: [AppleVzSharedDirectorySpec]
  /// Optional Unix domain socket used by the windowed display path for live
  /// status/stop/policy control. This is intentionally separate from the daemon socket:
  /// the AppKit display helper owns the actual VZ VM object.
  public var runtimeControlSocketPath: String?
  /// Optional raw RGBA framebuffer export path for the windowed display path.
  /// When set, the AppKit display helper periodically captures the
  /// `VZVirtualMachineView` into this file so host-side proxy-window crops can
  /// consume a real display source.
  public var proxyFramebufferRGBAPath: String?
  public var proxyFramebufferCaptureIntervalMillis: UInt64

  /// Host path of the first shared directory, or `nil` when none are configured.
  /// Retained for callers/tests that inspect a single share.
  public var sharedDirectoryPath: String? {
    sharedDirectorySpecs.first?.path
  }
  /// Mount tag of the first shared directory, or `nil` when none are configured.
  public var sharedDirectoryTag: String? {
    sharedDirectorySpecs.first?.tag
  }
  /// Read-only flag of the first shared directory (false when none configured).
  public var sharedDirectoryReadOnly: Bool {
    sharedDirectorySpecs.first?.readOnly ?? false
  }

  public init(
    stopAfterSeconds: UInt64? = nil,
    forceStopGraceSeconds: UInt64? = nil,
    saveStatePath: String? = nil,
    restoreStatePath: String? = nil,
    displayWindow: Bool = false,
    displayWidthInPixels: Int = 1280,
    displayHeightInPixels: Int = 800,
    graphicsHeadlessVerification: Bool = false,
    sharedDirectorySpecs: [AppleVzSharedDirectorySpec] = [],
    runtimeControlSocketPath: String? = nil,
    proxyFramebufferRGBAPath: String? = nil,
    proxyFramebufferCaptureIntervalMillis: UInt64 = 500
  ) {
    self.stopAfterSeconds = stopAfterSeconds
    self.forceStopGraceSeconds = forceStopGraceSeconds
    self.saveStatePath = saveStatePath
    self.restoreStatePath = restoreStatePath
    self.displayWindow = displayWindow
    self.displayWidthInPixels = displayWidthInPixels
    self.displayHeightInPixels = displayHeightInPixels
    self.graphicsHeadlessVerification = graphicsHeadlessVerification
    self.sharedDirectorySpecs = sharedDirectorySpecs
    self.runtimeControlSocketPath = runtimeControlSocketPath
    self.proxyFramebufferRGBAPath = proxyFramebufferRGBAPath
    self.proxyFramebufferCaptureIntervalMillis = proxyFramebufferCaptureIntervalMillis
  }

  /// Convenience initializer for a single shared directory (used by callers/tests
  /// that thread one path/tag through). A `nil` path yields no shares.
  public init(
    stopAfterSeconds: UInt64? = nil,
    forceStopGraceSeconds: UInt64? = nil,
    saveStatePath: String? = nil,
    restoreStatePath: String? = nil,
    displayWindow: Bool = false,
    displayWidthInPixels: Int = 1280,
    displayHeightInPixels: Int = 800,
    graphicsHeadlessVerification: Bool = false,
    sharedDirectoryPath: String?,
    sharedDirectoryTag: String? = nil,
    sharedDirectoryReadOnly: Bool = false,
    runtimeControlSocketPath: String? = nil,
    proxyFramebufferRGBAPath: String? = nil,
    proxyFramebufferCaptureIntervalMillis: UInt64 = 500
  ) {
    var specs: [AppleVzSharedDirectorySpec] = []
    if let path = sharedDirectoryPath {
      specs = [
        AppleVzSharedDirectorySpec(
          path: path,
          tag: sharedDirectoryTag ?? "share",
          readOnly: sharedDirectoryReadOnly
        )
      ]
    }
    self.init(
      stopAfterSeconds: stopAfterSeconds,
      forceStopGraceSeconds: forceStopGraceSeconds,
      saveStatePath: saveStatePath,
      restoreStatePath: restoreStatePath,
      displayWindow: displayWindow,
      displayWidthInPixels: displayWidthInPixels,
      displayHeightInPixels: displayHeightInPixels,
      graphicsHeadlessVerification: graphicsHeadlessVerification,
      sharedDirectorySpecs: specs,
      runtimeControlSocketPath: runtimeControlSocketPath,
      proxyFramebufferRGBAPath: proxyFramebufferRGBAPath,
      proxyFramebufferCaptureIntervalMillis: proxyFramebufferCaptureIntervalMillis
    )
  }
}

public protocol AppleVzVirtualMachineControlling: AnyObject {
  var canRequestStop: Bool { get }
  var canStop: Bool { get }

  func start(completionHandler: @escaping (Result<Void, Error>) -> Void)
  func requestStop() throws
  func stop(completionHandler: @escaping (Result<Void, Error>) -> Void)
  func setStopHandler(_ handler: @escaping (Result<Void, Error>) -> Void)
}

public final class AppleVzVirtualMachineLauncher {
  private let machine: AppleVzVirtualMachineControlling
  private let clockSleep: (UInt64) async -> Void

  public init(
    machine: AppleVzVirtualMachineControlling,
    clockSleep: @escaping (UInt64) async -> Void = { seconds in
      try? await Task.sleep(for: .seconds(seconds))
    }
  ) {
    self.machine = machine
    self.clockSleep = clockSleep
  }

  public func startAndWaitForStop(
    startMachine: Bool = true,
    interruptionSignals: AsyncStream<Void> = AppleVzSignalSource.sigint(),
    forceStopGraceSeconds: UInt64? = nil
  ) async throws -> AppleVzVirtualMachineLaunchResult {
    var wasInterrupted = false
    var didScheduleForceStop = false
    var forceStopTask: Task<Void, Never>?
    let eventSink = LaunchEventSink()
    defer {
      forceStopTask?.cancel()
    }
    let events = AsyncStream<LaunchEvent> { continuation in
      eventSink.setContinuation(continuation)
      machine.setStopHandler { result in
        log("VM stop handler fired: \(result)")
        continuation.yield(.stopped(result))
        continuation.finish()
      }

      if startMachine {
        log("VM start requested")
        machine.start { result in
          log("VM start completion: \(result)")
          continuation.yield(.started(result))
          if case .failure = result {
            continuation.finish()
          }
        }
      }

      let interruptionTask = Task {
        var iterator = interruptionSignals.makeAsyncIterator()
        if await iterator.next() != nil {
          continuation.yield(.interrupted)
        }
      }

      continuation.onTermination = { _ in
        interruptionTask.cancel()
      }
    }

    for await event in events {
      switch event {
      case .started(let result):
        try result.get()
      case .interrupted:
        log("VM interruption requested")
        wasInterrupted = true
        let requestedGuestStop = try await stopAfterInterruption()
        if requestedGuestStop, !didScheduleForceStop, let forceStopGraceSeconds {
          didScheduleForceStop = true
          log("VM force stop scheduled after \(forceStopGraceSeconds)s")
          forceStopTask = Task {
            await clockSleep(forceStopGraceSeconds)
            if !Task.isCancelled {
              log("VM force stop grace elapsed")
              do {
                try await forceStop()
                eventSink.yield(.stopped(.success(())))
              } catch {
                eventSink.yield(.stopped(.failure(error)))
              }
              eventSink.finish()
            }
          }
        }
      case .stopped(let result):
        forceStopTask?.cancel()
        try result.get()
        return wasInterrupted ? .interrupted : .stopped
      }
    }

    return wasInterrupted ? .interrupted : .stopped
  }

  private func stopAfterInterruption() async throws -> Bool {
    if machine.canRequestStop {
      log("VM guest stop requested")
      try machine.requestStop()
      return true
    }

    guard machine.canStop else {
      log("VM cannot request guest stop or immediate force stop yet")
      return true
    }

    log("VM force stop requested immediately")
    try await forceStop()
    return false
  }

  private func forceStop() async throws {
    log("VM force stop requested")
    try await withCheckedThrowingContinuation { continuation in
      machine.stop { result in
        log("VM force stop completion: \(result)")
        continuation.resume(with: result)
      }
    }
  }
}

private func log(_ message: String) {
  let line = "AppleVzRunner: \(message)\n"
  if let data = line.data(using: .utf8) {
    FileHandle.standardError.write(data)
  }
}

private final class LaunchEventSink {
  private let lock = NSLock()
  private var continuation: AsyncStream<LaunchEvent>.Continuation?

  func setContinuation(_ continuation: AsyncStream<LaunchEvent>.Continuation) {
    lock.withLock {
      self.continuation = continuation
    }
  }

  func yield(_ event: LaunchEvent) {
    _ = lock.withLock {
      continuation?.yield(event)
    }
  }

  func finish() {
    lock.withLock {
      continuation?.finish()
    }
  }
}

private final class MergedSignalSink: @unchecked Sendable {
  private let lock = NSLock()
  private let continuation: AsyncStream<Void>.Continuation
  private var didYield = false
  private var tasks: [Task<Void, Never>] = []

  init(continuation: AsyncStream<Void>.Continuation) {
    self.continuation = continuation
  }

  func setTasks(_ tasks: [Task<Void, Never>]) {
    let shouldCancel = lock.withLock {
      self.tasks = tasks
      return didYield
    }
    if shouldCancel {
      cancelTasks(tasks)
    }
  }

  func yieldAndFinish() {
    let tasksToCancel = lock.withLock {
      guard !didYield else {
        return [Task<Void, Never>]()
      }
      didYield = true
      return tasks
    }
    continuation.yield()
    continuation.finish()
    cancelTasks(tasksToCancel)
  }

  func cancel() {
    let tasksToCancel = lock.withLock {
      tasks
    }
    cancelTasks(tasksToCancel)
  }

  private func cancelTasks(_ tasks: [Task<Void, Never>]) {
    for task in tasks {
      task.cancel()
    }
  }
}

private enum LaunchEvent {
  case started(Result<Void, Error>)
  case interrupted
  case stopped(Result<Void, Error>)
}

public enum AppleVzSignalSource {
  public static func sigint() -> AsyncStream<Void> {
    #if canImport(Darwin)
    AsyncStream { continuation in
      let source = DispatchSource.makeSignalSource(signal: SIGINT, queue: .global())
      let previousHandler = signal(SIGINT, SIG_IGN)
      source.setEventHandler {
        continuation.yield()
      }
      source.setCancelHandler {
        signal(SIGINT, previousHandler)
      }
      continuation.onTermination = { _ in
        source.cancel()
      }
      source.resume()
    }
    #else
    AsyncStream { continuation in
      continuation.finish()
    }
    #endif
  }

  public static func timer(afterSeconds seconds: UInt64?) -> AsyncStream<Void> {
    guard let seconds else {
      return AsyncStream { _ in }
    }

    return AsyncStream { continuation in
      let task = Task {
        try? await Task.sleep(for: .seconds(seconds))
        guard !Task.isCancelled else {
          return
        }
        continuation.yield()
        continuation.finish()
      }
      continuation.onTermination = { _ in
        task.cancel()
      }
    }
  }

  public static func merged(_ streams: [AsyncStream<Void>]) -> AsyncStream<Void> {
    AsyncStream { continuation in
      let sink = MergedSignalSink(continuation: continuation)
      let tasks = streams.map { stream in
        Task {
          var iterator = stream.makeAsyncIterator()
          if await iterator.next() != nil {
            sink.yieldAndFinish()
          }
        }
      }
      sink.setTasks(tasks)
      continuation.onTermination = { _ in
        sink.cancel()
      }
    }
  }
}

#if canImport(Darwin)
struct AppleVzDisplayRuntimeControlSnapshot: Equatable {
  var vmName: String
  var state: String
  var displayWidthInPixels: Int
  var displayHeightInPixels: Int
  var isStopping: Bool
  var proxyFramebufferRGBAPath: String? = nil
  var proxyFramebufferCaptureIntervalMillis: UInt64? = nil
}

enum AppleVzDisplayRuntimeControlServerError: Error, LocalizedError, Equatable {
  case socketPathTooLong(String)
  case socketCreateFailed(String)
  case socketPathNotSocket(String)
  case socketAlreadyInUse(String)
  case socketProbeFailed(String)
  case bindFailed(String)
  case listenFailed(String)
  case permissionsFailed(String)

  var errorDescription: String? {
    switch self {
    case .socketPathTooLong(let path):
      return "runtime control socket path is too long: \(path)"
    case .socketCreateFailed(let message):
      return "runtime control socket create failed: \(message)"
    case .socketPathNotSocket(let path):
      return "runtime control socket path exists and is not a socket: \(path)"
    case .socketAlreadyInUse(let path):
      return "runtime control socket is already in use: \(path)"
    case .socketProbeFailed(let message):
      return "runtime control socket probe failed: \(message)"
    case .bindFailed(let message):
      return "runtime control socket bind failed: \(message)"
    case .listenFailed(let message):
      return "runtime control socket listen failed: \(message)"
    case .permissionsFailed(let message):
      return "runtime control socket permissions failed: \(message)"
    }
  }
}

final class AppleVzDisplayRuntimeControlServer {
  private static let maximumRequestBytes = 4096
  private static let clientIOTimeoutSeconds: Int = 2
  private static let maximumConcurrentClients = 8

  private struct SocketIdentity {
    let device: dev_t
    let inode: ino_t
  }

  private let socketPath: String
  private let statusProvider: () -> AppleVzDisplayRuntimeControlSnapshot
  private let stopHandler: () -> Void
  private let runtimePolicyProvider: () -> [String: Any]?
  private let queue = DispatchQueue(label: "com.bridgevm.apple-vz.display-runtime-control")
  private let clientQueue = DispatchQueue(
    label: "com.bridgevm.apple-vz.display-runtime-control.clients",
    attributes: .concurrent
  )
  private let clientSlots = DispatchSemaphore(value: maximumConcurrentClients)
  private var socketFD: Int32 = -1
  private var source: DispatchSourceRead?

  init(
    socketPath: String,
    statusProvider: @escaping () -> AppleVzDisplayRuntimeControlSnapshot,
    stopHandler: @escaping () -> Void,
    runtimePolicyProvider: @escaping () -> [String: Any]? = { nil }
  ) {
    self.socketPath = socketPath
    self.statusProvider = statusProvider
    self.stopHandler = stopHandler
    self.runtimePolicyProvider = runtimePolicyProvider
  }

  func start() throws {
    let url = URL(fileURLWithPath: socketPath)
    try FileManager.default.createDirectory(
      at: url.deletingLastPathComponent(),
      withIntermediateDirectories: true
    )
    try prepareSocketPath(url)

    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    guard fd >= 0 else {
      throw AppleVzDisplayRuntimeControlServerError.socketCreateFailed(lastErrnoMessage())
    }

    var socketIdentity: SocketIdentity?
    do {
      try bindSocket(fd)
      var info = stat()
      guard fstat(fd, &info) == 0 else {
        throw AppleVzDisplayRuntimeControlServerError.bindFailed(lastErrnoMessage())
      }
      socketIdentity = SocketIdentity(device: info.st_dev, inode: info.st_ino)
      guard chmod(socketPath, S_IRUSR | S_IWUSR) == 0 else {
        throw AppleVzDisplayRuntimeControlServerError.permissionsFailed(lastErrnoMessage())
      }
      guard listen(fd, 8) == 0 else {
        throw AppleVzDisplayRuntimeControlServerError.listenFailed(lastErrnoMessage())
      }
    } catch {
      close(fd)
      if let socketIdentity {
        Self.removeSocket(atPath: socketPath, matching: socketIdentity)
      }
      throw error
    }

    socketFD = fd
    let source = DispatchSource.makeReadSource(fileDescriptor: fd, queue: queue)
    source.setEventHandler { [weak self] in
      self?.acceptAvailableConnections()
    }
    let ownedSocketIdentity = socketIdentity!
    source.setCancelHandler { [socketPath] in
      close(fd)
      Self.removeSocket(atPath: socketPath, matching: ownedSocketIdentity)
    }
    self.source = source
    source.resume()
  }

  private static func removeSocket(atPath path: String, matching identity: SocketIdentity) {
    var info = stat()
    guard lstat(path, &info) == 0,
      (info.st_mode & S_IFMT) == S_IFSOCK,
      info.st_dev == identity.device,
      info.st_ino == identity.inode
    else {
      return
    }
    _ = unlink(path)
  }

  private func prepareSocketPath(_ url: URL) throws {
    var info = stat()
    guard lstat(socketPath, &info) == 0 else {
      if errno == ENOENT {
        return
      }
      throw AppleVzDisplayRuntimeControlServerError.bindFailed(lastErrnoMessage())
    }
    guard (info.st_mode & S_IFMT) == S_IFSOCK else {
      throw AppleVzDisplayRuntimeControlServerError.socketPathNotSocket(socketPath)
    }

    let probe = socket(AF_UNIX, SOCK_STREAM, 0)
    guard probe >= 0 else {
      throw AppleVzDisplayRuntimeControlServerError.socketProbeFailed(lastErrnoMessage())
    }
    defer { close(probe) }
    if try connectSocket(probe) {
      throw AppleVzDisplayRuntimeControlServerError.socketAlreadyInUse(socketPath)
    }
    try FileManager.default.removeItem(at: url)
  }

  private func connectSocket(_ fd: Int32) throws -> Bool {
    var address = sockaddr_un()
    address.sun_family = sa_family_t(AF_UNIX)
    let pathBytes = Array(socketPath.utf8)
    let capacity = MemoryLayout.size(ofValue: address.sun_path)
    guard pathBytes.count < capacity else {
      throw AppleVzDisplayRuntimeControlServerError.socketPathTooLong(socketPath)
    }
    withUnsafeMutablePointer(to: &address.sun_path) { pointer in
      pointer.withMemoryRebound(to: CChar.self, capacity: capacity) { path in
        path.initialize(repeating: 0, count: capacity)
        for (index, byte) in pathBytes.enumerated() {
          path[index] = CChar(bitPattern: byte)
        }
      }
    }
    let result = withUnsafePointer(to: &address) { pointer in
      pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
        connect(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
      }
    }
    guard result == 0 else {
      let connectionError = errno
      if connectionError == ECONNREFUSED {
        return false
      }
      throw AppleVzDisplayRuntimeControlServerError.socketProbeFailed(
        String(cString: strerror(connectionError))
      )
    }
    return true
  }

  func stop() {
    source?.cancel()
    source = nil
    socketFD = -1
  }

  private func bindSocket(_ fd: Int32) throws {
    var address = sockaddr_un()
    address.sun_family = sa_family_t(AF_UNIX)
    let pathBytes = Array(socketPath.utf8)
    let sunPathCapacity = MemoryLayout.size(ofValue: address.sun_path)
    guard pathBytes.count < sunPathCapacity else {
      throw AppleVzDisplayRuntimeControlServerError.socketPathTooLong(socketPath)
    }

    withUnsafeMutablePointer(to: &address.sun_path) { pointer in
      pointer.withMemoryRebound(to: CChar.self, capacity: sunPathCapacity) { sunPath in
        for index in 0..<sunPathCapacity {
          sunPath[index] = 0
        }
        for (index, byte) in pathBytes.enumerated() {
          sunPath[index] = CChar(bitPattern: byte)
        }
      }
    }

    let result = withUnsafePointer(to: &address) { pointer in
      pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
        bind(fd, sockaddrPointer, socklen_t(MemoryLayout<sockaddr_un>.size))
      }
    }
    guard result == 0 else {
      throw AppleVzDisplayRuntimeControlServerError.bindFailed(lastErrnoMessage())
    }
  }

  private func acceptAvailableConnections() {
    let client = accept(socketFD, nil, nil)
    if client >= 0 {
      guard clientSlots.wait(timeout: .now()) == .success else {
        close(client)
        return
      }
      clientQueue.async { [self] in
        defer { clientSlots.signal() }
        self.handle(client)
      }
    }
  }

  private func handle(_ client: Int32) {
    defer {
      close(client)
    }

    var suppressSIGPIPE: Int32 = 1
    _ = setsockopt(
      client,
      SOL_SOCKET,
      SO_NOSIGPIPE,
      &suppressSIGPIPE,
      socklen_t(MemoryLayout<Int32>.size)
    )
    var timeout = timeval(tv_sec: Self.clientIOTimeoutSeconds, tv_usec: 0)
    _ = setsockopt(
      client,
      SOL_SOCKET,
      SO_RCVTIMEO,
      &timeout,
      socklen_t(MemoryLayout<timeval>.size)
    )
    _ = setsockopt(
      client,
      SOL_SOCKET,
      SO_SNDTIMEO,
      &timeout,
      socklen_t(MemoryLayout<timeval>.size)
    )

    guard let request = readRequest(from: client) else {
      return
    }
    let response: Data
    if request.count > Self.maximumRequestBytes {
      response = encode(["ok": false, "error": "request-too-large"])
    } else {
      response = handleRequest(request)
    }
    writeResponse(response, to: client)
  }

  private func readRequest(from client: Int32) -> Data? {
    var request = Data()
    var buffer = [UInt8](repeating: 0, count: 512)
    while request.count <= Self.maximumRequestBytes {
      let count = read(client, &buffer, buffer.count)
      guard count > 0 else {
        return nil
      }
      if let newline = buffer[..<count].firstIndex(of: 0x0A) {
        request.append(contentsOf: buffer[..<newline])
        return request
      }
      request.append(contentsOf: buffer[..<count])
    }
    return request
  }

  private func writeResponse(_ response: Data, to client: Int32) {
    response.withUnsafeBytes { rawBuffer in
      guard let baseAddress = rawBuffer.baseAddress else { return }
      var written = 0
      while written < response.count {
        let count = write(client, baseAddress.advanced(by: written), response.count - written)
        guard count > 0 else { return }
        written += count
      }
    }
  }

  private func handleRequest(_ data: Data) -> Data {
    let command = parseCommand(data)
    switch command {
    case "status":
      return encode(statusResponse())
    case "stop":
      stopHandler()
      var response = statusResponse()
      response["accepted"] = true
      return encode(response)
    case "policy":
      return encode(policyResponse())
    case "pacing":
      return encode(pacingResponse())
    default:
      return encode([
        "ok": false,
        "error": "unknown-command",
        "supported_commands": supportedCommands,
      ])
    }
  }

  private func parseCommand(_ data: Data) -> String? {
    guard
      let object = try? JSONSerialization.jsonObject(with: data),
      let dictionary = object as? [String: Any],
      let command = dictionary["command"] as? String
    else {
      return nil
    }
    return command
  }

  private func statusResponse() -> [String: Any] {
    let snapshot = statusProvider()
    var framebufferExport: [String: Any] = [
      "enabled": snapshot.proxyFramebufferRGBAPath != nil
    ]
    if let path = snapshot.proxyFramebufferRGBAPath {
      framebufferExport["path"] = path
    }
    if let intervalMillis = snapshot.proxyFramebufferCaptureIntervalMillis {
      framebufferExport["interval_millis"] = intervalMillis
    }
    return [
      "ok": true,
      "vm": snapshot.vmName,
      "state": snapshot.state,
      "stopping": snapshot.isStopping,
      "display": [
        "width": snapshot.displayWidthInPixels,
        "height": snapshot.displayHeightInPixels,
      ],
      "framebuffer_export": framebufferExport,
      "runtime_policy": [
        "available": runtimePolicyProvider() != nil
      ],
      "supported_commands": supportedCommands,
    ]
  }

  private func policyResponse() -> [String: Any] {
    guard let policy = runtimePolicyProvider() else {
      return [
        "ok": false,
        "error": "policy-unavailable",
        "supported_commands": supportedCommands,
      ]
    }
    return [
      "ok": true,
      "policy": policy,
      "supported_commands": supportedCommands,
    ]
  }

  private func pacingResponse() -> [String: Any] {
    guard let policy = runtimePolicyProvider() else {
      return [
        "ok": false,
        "error": "policy-unavailable",
        "supported_commands": supportedCommands,
      ]
    }

    let visibility = stringValue(policy["visibility"]) ?? "unknown"
    let displayFPSCap = stringValue(policy["display_fps_cap"]) ?? "adaptive"
    let maxFPS: Any
    if let numericCap = Int(displayFPSCap) {
      maxFPS = numericCap
    } else {
      maxFPS = displayFPSCap
    }

    return [
      "ok": true,
      "visibility": visibility,
      "display_fps_cap": displayFPSCap,
      "max_fps": maxFPS,
      "policy_available": true,
      "supported_commands": supportedCommands,
    ]
  }

  private func stringValue(_ value: Any?) -> String? {
    switch value {
    case let value as String:
      return value
    case let value as CustomStringConvertible:
      return value.description
    default:
      return nil
    }
  }

  private var supportedCommands: [String] {
    ["status", "stop", "policy", "pacing"]
  }

  private func encode(_ object: [String: Any]) -> Data {
    let data = (try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys]))
      ?? Data(#"{"ok":false,"error":"encode-failed"}"#.utf8)
    var output = data
    output.append(0x0A)
    return output
  }
}

private func lastErrnoMessage() -> String {
  String(cString: strerror(errno))
}
#endif

#if canImport(Virtualization)
@available(macOS 12.0, *)
public extension AppleVzVirtualMachineLauncher {
  static func launchLinuxKernelVirtualMachine(
    spec: AppleVzLaunchSpec,
    options: AppleVzLaunchOptions = AppleVzLaunchOptions()
  ) throws {
    let configuration: VZVirtualMachineConfiguration
    if options.graphicsHeadlessVerification {
      // Verification path: boot the graphics-device config headless (no window)
      // to prove a real guest comes up with the Virtio GPU attached.
      guard #available(macOS 14.0, *) else {
        throw AppleVzRunnerCommandError.displayRequiresMacOS14
      }
      configuration = try AppleVzConfigurationBuilder.buildLinuxKernelConfigurationWithDisplay(
        spec: spec,
        widthInPixels: options.displayWidthInPixels,
        heightInPixels: options.displayHeightInPixels,
        sharedDirectories: options.sharedDirectorySpecs)
    } else {
      configuration = try AppleVzConfigurationBuilder.buildLinuxKernelConfiguration(
        spec: spec, sharedDirectories: options.sharedDirectorySpecs)
    }
    try configuration.validate()

    print("AppleVzRunner starting VM: \(spec.vmName)")
    let machine = AppleVzVirtualMachineAdapter(configuration: configuration)
    let launcher = AppleVzVirtualMachineLauncher(machine: machine)
    let semaphore = DispatchSemaphore(value: 0)
    var launchResult: Result<AppleVzVirtualMachineLaunchResult, Error>?

    Task {
      do {
        launchResult = .success(
          try await launcher.startAndWaitForStop(
            interruptionSignals: AppleVzSignalSource.merged([
              AppleVzSignalSource.sigint(),
              AppleVzSignalSource.timer(afterSeconds: options.stopAfterSeconds),
            ]),
            forceStopGraceSeconds: options.forceStopGraceSeconds
          )
        )
      } catch {
        launchResult = .failure(error)
      }
      semaphore.signal()
    }

    semaphore.wait()
    let result = try launchResult?.get()
    print("AppleVzRunner VM finished: \(spec.vmName) (\(result ?? .stopped))")
  }

  /// Boot the Linux VM, run for `afterSeconds`, then pause and write the full
  /// machine state (memory + devices) to `statePath` for later resume.
  @available(macOS 14.0, *)
  static func suspendLinuxKernelVirtualMachine(
    spec: AppleVzLaunchSpec,
    afterSeconds: UInt64,
    toStatePath statePath: String
  ) throws {
    let configuration = try AppleVzConfigurationBuilder.buildLinuxKernelConfiguration(spec: spec)
    try configuration.validate()
    try assertSaveRestoreSupported(configuration)

    print("AppleVzRunner starting VM to suspend: \(spec.vmName)")
    let machine = AppleVzVirtualMachineAdapter(configuration: configuration)
    let semaphore = DispatchSemaphore(value: 0)
    var flowResult: Result<Void, Error>?

    Task {
      do {
        try await awaitVZVoid { machine.start(completionHandler: $0) }
        print("AppleVzRunner VM running; will suspend after \(afterSeconds)s")
        try? await Task.sleep(for: .seconds(afterSeconds))
        try await awaitVZVoid { machine.pause(completionHandler: $0) }
        print("AppleVzRunner VM paused; saving machine state to \(statePath)")
        try await awaitVZVoid {
          machine.saveState(to: URL(fileURLWithPath: statePath), completionHandler: $0)
        }
        print("AppleVzRunner saved machine state: \(statePath)")
        try? await awaitVZVoid { machine.stop(completionHandler: $0) }
        flowResult = .success(())
      } catch {
        // Best-effort cleanup: stop the still-running VM and remove any partial
        // state file so a later --restore-state cannot consume a corrupt save.
        try? await awaitVZVoid { machine.stop(completionHandler: $0) }
        try? FileManager.default.removeItem(at: URL(fileURLWithPath: statePath))
        flowResult = .failure(error)
      }
      semaphore.signal()
    }

    semaphore.wait()
    try flowResult?.get()
    print("AppleVzRunner suspend complete: \(spec.vmName)")
  }

  /// Restore the Linux VM from a previously saved state file and resume it,
  /// running until `options.stopAfterSeconds` elapses (or SIGINT).
  @available(macOS 14.0, *)
  static func restoreLinuxKernelVirtualMachine(
    spec: AppleVzLaunchSpec,
    fromStatePath statePath: String,
    options: AppleVzLaunchOptions = AppleVzLaunchOptions()
  ) throws {
    let configuration = try AppleVzConfigurationBuilder.buildLinuxKernelConfiguration(spec: spec)
    try configuration.validate()
    try assertSaveRestoreSupported(configuration)

    print("AppleVzRunner restoring VM: \(spec.vmName) from \(statePath)")
    let machine = AppleVzVirtualMachineAdapter(configuration: configuration)
    let launcher = AppleVzVirtualMachineLauncher(machine: machine)
    let semaphore = DispatchSemaphore(value: 0)
    var flowResult: Result<Void, Error>?

    Task {
      do {
        try await awaitVZVoid {
          machine.restoreState(from: URL(fileURLWithPath: statePath), completionHandler: $0)
        }
        print("AppleVzRunner restored machine state; resuming")
        try await awaitVZVoid { machine.resume(completionHandler: $0) }
        print("AppleVzRunner VM resumed: \(spec.vmName)")
        // Reuse the robust wait loop (guest-stop handler + interruption + force
        // stop grace) without re-starting the already-resumed machine.
        _ = try await launcher.startAndWaitForStop(
          startMachine: false,
          interruptionSignals: AppleVzSignalSource.merged([
            AppleVzSignalSource.sigint(),
            AppleVzSignalSource.timer(afterSeconds: options.stopAfterSeconds),
          ]),
          forceStopGraceSeconds: options.forceStopGraceSeconds
        )
        flowResult = .success(())
      } catch {
        try? await awaitVZVoid { machine.stop(completionHandler: $0) }
        flowResult = .failure(error)
      }
      semaphore.signal()
    }

    semaphore.wait()
    try flowResult?.get()
    print("AppleVzRunner restore complete: \(spec.vmName)")
  }
}

@available(macOS 12.0, *)
fileprivate func awaitVZVoid(
  _ body: (@escaping (Result<Void, Error>) -> Void) -> Void
) async throws {
  try await withCheckedThrowingContinuation { continuation in
    body { result in continuation.resume(with: result) }
  }
}

@available(macOS 14.0, *)
fileprivate func assertSaveRestoreSupported(
  _ configuration: VZVirtualMachineConfiguration
) throws {
  do {
    try configuration.validateSaveRestoreSupport()
    print("AppleVzRunner save/restore support: OK")
  } catch {
    print("AppleVzRunner save/restore support unavailable: \(error)")
    throw error
  }
}

@available(macOS 12.0, *)
public final class AppleVzVirtualMachineAdapter: NSObject, AppleVzVirtualMachineControlling,
  VZVirtualMachineDelegate
{
  private let virtualMachine: VZVirtualMachine
  private let queue: DispatchQueue
  private let queueKey = DispatchSpecificKey<Void>()
  private var stopHandler: ((Result<Void, Error>) -> Void)?

  public init(configuration: VZVirtualMachineConfiguration) {
    let queue = DispatchQueue(label: "com.bridgevm.apple-vz.virtual-machine")
    self.queue = queue
    self.queue.setSpecific(key: queueKey, value: ())
    self.virtualMachine = VZVirtualMachine(configuration: configuration, queue: queue)
    super.init()
    self.virtualMachine.delegate = self
  }

  public var canRequestStop: Bool {
    onQueue {
      virtualMachine.canRequestStop
    }
  }

  public var canStop: Bool {
    onQueue {
      virtualMachine.canStop
    }
  }

  public func start(completionHandler: @escaping (Result<Void, Error>) -> Void) {
    queue.async { [virtualMachine] in
      virtualMachine.start(completionHandler: completionHandler)
    }
  }

  public func requestStop() throws {
    try onQueue {
      try virtualMachine.requestStop()
    }
  }

  public func stop(completionHandler: @escaping (Result<Void, Error>) -> Void) {
    queue.async { [virtualMachine] in
      virtualMachine.stop { error in
        if let error {
          completionHandler(.failure(error))
        } else {
          completionHandler(.success(()))
        }
      }
    }
  }

  public func setStopHandler(_ handler: @escaping (Result<Void, Error>) -> Void) {
    onQueue {
      stopHandler = handler
    }
  }

  public var canPause: Bool {
    onQueue { virtualMachine.canPause }
  }

  public var canResume: Bool {
    onQueue { virtualMachine.canResume }
  }

  public func pause(completionHandler: @escaping (Result<Void, Error>) -> Void) {
    queue.async { [virtualMachine] in
      virtualMachine.pause(completionHandler: completionHandler)
    }
  }

  public func resume(completionHandler: @escaping (Result<Void, Error>) -> Void) {
    queue.async { [virtualMachine] in
      virtualMachine.resume(completionHandler: completionHandler)
    }
  }

  @available(macOS 14.0, *)
  public func saveState(to url: URL, completionHandler: @escaping (Result<Void, Error>) -> Void) {
    queue.async { [virtualMachine] in
      virtualMachine.saveMachineStateTo(url: url) { error in
        completionHandler(error.map { .failure($0) } ?? .success(()))
      }
    }
  }

  @available(macOS 14.0, *)
  public func restoreState(from url: URL, completionHandler: @escaping (Result<Void, Error>) -> Void) {
    queue.async { [virtualMachine] in
      virtualMachine.restoreMachineStateFrom(url: url) { error in
        completionHandler(error.map { .failure($0) } ?? .success(()))
      }
    }
  }

  public func guestDidStop(_ virtualMachine: VZVirtualMachine) {
    onQueue {
      stopHandler?(.success(()))
    }
  }

  public func virtualMachine(_ virtualMachine: VZVirtualMachine, didStopWithError error: Error) {
    onQueue {
      stopHandler?(.failure(error))
    }
  }

  private func onQueue<T>(_ operation: () throws -> T) rethrows -> T {
    if DispatchQueue.getSpecific(key: queueKey) != nil {
      return try operation()
    }
    return try queue.sync {
      try operation()
    }
  }
}

public enum AppleVzVirtualMachineAdapterError: Error, LocalizedError, Equatable {
  case stopRequestRejected

  public var errorDescription: String? {
    switch self {
    case .stopRequestRejected:
      return "AppleVzRunner could not request the guest to stop"
    }
  }
}
#endif

// MARK: - Embedded graphical display (windowed)
//
// Hosts the running Linux VM in an on-screen `VZVirtualMachineView`. This is a
// deliberately separate path from the headless launcher above: `VZVirtualMachineView`
// requires the VM to live on the main queue, so this path creates the VM with the
// main-queue initializer and drives it from an AppKit run loop rather than the
// background-queue adapter. The headless boot + save/restore path is untouched, so a
// graphics device (which generally disables VZ save/restore) never affects it.
#if canImport(Virtualization) && canImport(AppKit)
import AppKit

/// Retains the window controller for the lifetime of the app run loop
/// (`NSApplication.delegate` does not keep a strong reference).
private var displayWindowControllerRetainer: AnyObject?

@available(macOS 14.0, *)
public extension AppleVzVirtualMachineLauncher {
  /// Boot the Linux VM with a graphics device and show it in a resizable window
  /// hosting a `VZVirtualMachineView`. Blocks in the AppKit run loop until the
  /// window is closed or the guest stops. Requires a GUI session (cannot run
  /// headless); intended to be spawned by the macOS app as a helper process.
  static func launchLinuxKernelVirtualMachineWithDisplay(
    spec: AppleVzLaunchSpec,
    options: AppleVzLaunchOptions = AppleVzLaunchOptions()
  ) throws {
    let configuration = try AppleVzConfigurationBuilder.buildLinuxKernelConfigurationWithDisplay(
      spec: spec,
      widthInPixels: options.displayWidthInPixels,
      heightInPixels: options.displayHeightInPixels,
      sharedDirectories: options.sharedDirectorySpecs)
    try configuration.validate()

    print("AppleVzRunner starting VM with display: \(spec.vmName)")
    let app = NSApplication.shared
    app.setActivationPolicy(.regular)

    // Main-queue VM (required by VZVirtualMachineView).
    let machine = VZVirtualMachine(configuration: configuration)
    let controller = AppleVzDisplayWindowController(
      machine: machine,
      vmName: spec.vmName,
      app: app,
      displayWidthInPixels: options.displayWidthInPixels,
      displayHeightInPixels: options.displayHeightInPixels,
      stopAfterSeconds: options.stopAfterSeconds,
      forceStopGraceSeconds: options.forceStopGraceSeconds,
      runtimeControlSocketPath: options.runtimeControlSocketPath,
      runtimePolicyPath: runtimeResourcePolicyPath(bundlePath: spec.bundlePath),
      proxyFramebufferRGBAPath: options.proxyFramebufferRGBAPath,
      proxyFramebufferCaptureIntervalMillis: options.proxyFramebufferCaptureIntervalMillis
    )
    displayWindowControllerRetainer = controller
    app.delegate = controller

    app.activate(ignoringOtherApps: true)
    app.run()
  }
}

@available(macOS 14.0, *)
final class AppleVzDisplayWindowController: NSObject, NSApplicationDelegate,
  VZVirtualMachineDelegate
{
  private let machine: VZVirtualMachine
  private let vmName: String
  private unowned let app: NSApplication
  private let displayWidthInPixels: Int
  private let displayHeightInPixels: Int
  private let stopAfterSeconds: UInt64?
  private let forceStopGraceSeconds: UInt64?
  private let runtimeControlSocketPath: String?
  private let runtimePolicyPath: String?
  private let proxyFramebufferRGBAPath: String?
  private let proxyFramebufferCaptureIntervalMillis: UInt64
  private var window: NSWindow?
  private var signalSources: [DispatchSourceSignal] = []
  private var runtimeControlServer: AppleVzDisplayRuntimeControlServer?
  private var framebufferExporter: AppleVzDisplayFramebufferExporter?
  private var runtimeState = "starting"
  private var isStopping = false

  init(
    machine: VZVirtualMachine,
    vmName: String,
    app: NSApplication,
    displayWidthInPixels: Int,
    displayHeightInPixels: Int,
    stopAfterSeconds: UInt64? = nil,
    forceStopGraceSeconds: UInt64? = nil,
    runtimeControlSocketPath: String? = nil,
    runtimePolicyPath: String? = nil,
    proxyFramebufferRGBAPath: String? = nil,
    proxyFramebufferCaptureIntervalMillis: UInt64 = 500
  ) {
    self.machine = machine
    self.vmName = vmName
    self.app = app
    self.displayWidthInPixels = displayWidthInPixels
    self.displayHeightInPixels = displayHeightInPixels
    self.stopAfterSeconds = stopAfterSeconds
    self.forceStopGraceSeconds = forceStopGraceSeconds
    self.runtimeControlSocketPath = runtimeControlSocketPath
    self.runtimePolicyPath = runtimePolicyPath
    self.proxyFramebufferRGBAPath = proxyFramebufferRGBAPath
    self.proxyFramebufferCaptureIntervalMillis = proxyFramebufferCaptureIntervalMillis
    super.init()
    machine.delegate = self
  }

  func applicationDidFinishLaunching(_ notification: Notification) {
    let frame = NSRect(
      x: 0,
      y: 0,
      width: displayWidthInPixels,
      height: displayHeightInPixels
    )
    let view = VZVirtualMachineView(frame: frame)
    view.virtualMachine = machine
    view.capturesSystemKeys = true

    let window = NSWindow(
      contentRect: frame,
      styleMask: [.titled, .closable, .miniaturizable, .resizable],
      backing: .buffered,
      defer: false
    )
    window.title = "BridgeVM — \(vmName)"
    window.contentView = view
    window.center()
    window.makeKeyAndOrderFront(nil)
    self.window = window
    startFramebufferExporterIfNeeded(for: view)
    startRuntimeControlServerIfNeeded()

    machine.start { [weak self] result in
      if case let .failure(error) = result {
        FileHandle.standardError.write(
          Data("AppleVzRunner: display VM start failed: \(error)\n".utf8))
        self?.runtimeState = "error"
        self?.app.terminate(nil)
      } else {
        self?.runtimeState = "running"
        print("AppleVzRunner display VM running: \(self?.vmName ?? "")")
      }
    }

    // Honor --stop-after-seconds (automation) by gracefully stopping then.
    if let stopAfter = stopAfterSeconds {
      DispatchQueue.main.asyncAfter(deadline: .now() + Double(stopAfter)) { [weak self] in
        self?.beginGracefulStop()
      }
    }
    // SIGTERM/SIGINT (e.g. the daemon reaper) -> request a clean guest stop
    // before AppKit terminates, instead of being torn down abruptly.
    for sig in [SIGINT, SIGTERM] {
      signal(sig, SIG_IGN)
      let source = DispatchSource.makeSignalSource(signal: sig, queue: .main)
      source.setEventHandler { [weak self] in self?.beginGracefulStop() }
      source.resume()
      signalSources.append(source)
    }
  }

  /// Ask the guest to stop, then force-stop after a grace period, then exit.
  private func beginGracefulStop() {
    guard !isStopping else { return }
    isStopping = true
    runtimeState = "stopping"
    if machine.canRequestStop {
      do {
        try machine.requestStop()
      } catch {
        forceStopAndTerminate()
        return
      }
      let grace = forceStopGraceSeconds ?? 10
      DispatchQueue.main.asyncAfter(deadline: .now() + Double(grace)) { [weak self] in
        self?.forceStopAndTerminate()
      }
    } else {
      forceStopAndTerminate()
    }
  }

  private func forceStopAndTerminate() {
    if machine.canStop {
      machine.stop { [weak self] _ in self?.app.terminate(nil) }
    } else {
      app.terminate(nil)
    }
  }

  private func startFramebufferExporterIfNeeded(for view: NSView) {
    guard let proxyFramebufferRGBAPath else {
      return
    }
    let exporter = AppleVzDisplayFramebufferExporter(
      view: view,
      outputPath: proxyFramebufferRGBAPath,
      width: displayWidthInPixels,
      height: displayHeightInPixels,
      intervalMillis: proxyFramebufferCaptureIntervalMillis
    )
    do {
      try exporter.start()
      framebufferExporter = exporter
      print("AppleVzRunner display framebuffer export: \(proxyFramebufferRGBAPath)")
    } catch {
      FileHandle.standardError.write(
        Data("AppleVzRunner: display framebuffer export unavailable: \(error)\n".utf8))
    }
  }

  private func startRuntimeControlServerIfNeeded() {
    guard let runtimeControlSocketPath else {
      return
    }
    let server = AppleVzDisplayRuntimeControlServer(
      socketPath: runtimeControlSocketPath,
      statusProvider: { [weak self] in
        DispatchQueue.main.sync {
          self?.runtimeControlSnapshot()
            ?? AppleVzDisplayRuntimeControlSnapshot(
              vmName: "unknown",
              state: "stopped",
              displayWidthInPixels: 0,
              displayHeightInPixels: 0,
              isStopping: true,
              proxyFramebufferRGBAPath: nil,
              proxyFramebufferCaptureIntervalMillis: nil
            )
        }
      },
      stopHandler: { [weak self] in
        DispatchQueue.main.async {
          self?.beginGracefulStop()
        }
      },
      runtimePolicyProvider: { [weak self] in
        self?.readRuntimePolicy()
      }
    )
    do {
      try server.start()
      runtimeControlServer = server
      print("AppleVzRunner display runtime control socket: \(runtimeControlSocketPath)")
    } catch {
      FileHandle.standardError.write(
        Data("AppleVzRunner: display runtime control unavailable: \(error)\n".utf8))
    }
  }

  private func runtimeControlSnapshot() -> AppleVzDisplayRuntimeControlSnapshot {
    AppleVzDisplayRuntimeControlSnapshot(
      vmName: vmName,
      state: runtimeState,
      displayWidthInPixels: displayWidthInPixels,
      displayHeightInPixels: displayHeightInPixels,
      isStopping: isStopping,
      proxyFramebufferRGBAPath: proxyFramebufferRGBAPath,
      proxyFramebufferCaptureIntervalMillis: proxyFramebufferRGBAPath == nil
        ? nil : proxyFramebufferCaptureIntervalMillis
    )
  }

  private func readRuntimePolicy() -> [String: Any]? {
    guard let runtimePolicyPath else {
      return nil
    }
    guard let data = try? Data(contentsOf: URL(fileURLWithPath: runtimePolicyPath)) else {
      return nil
    }
    return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
  }

  func applicationWillTerminate(_ notification: Notification) {
    framebufferExporter?.stop()
    framebufferExporter = nil
    runtimeControlServer?.stop()
    runtimeControlServer = nil
  }

  func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    true
  }

  func guestDidStop(_ virtualMachine: VZVirtualMachine) {
    print("AppleVzRunner display VM guest stopped: \(vmName)")
    runtimeState = "stopped"
    app.terminate(nil)
  }

  func virtualMachine(_ virtualMachine: VZVirtualMachine, didStopWithError error: Error) {
    FileHandle.standardError.write(
      Data("AppleVzRunner: display VM stopped with error: \(error)\n".utf8))
    runtimeState = "error"
    app.terminate(nil)
  }
}

@available(macOS 14.0, *)
final class AppleVzDisplayFramebufferExporter {
  enum ExportError: Error, LocalizedError, Equatable {
    case invalidDimensions(width: Int, height: Int)
    case viewUnavailable
    case bitmapCreationFailed
    case cgImageCreationFailed
    case contextCreationFailed

    var errorDescription: String? {
      switch self {
      case let .invalidDimensions(width, height):
        return "framebuffer export dimensions are unsupported; got \(width)x\(height)"
      case .viewUnavailable:
        return "display view is no longer available"
      case .bitmapCreationFailed:
        return "could not allocate an AppKit bitmap for display capture"
      case .cgImageCreationFailed:
        return "could not convert display capture to CGImage"
      case .contextCreationFailed:
        return "could not allocate RGBA export context"
      }
    }
  }

  private weak var view: NSView?
  private let outputURL: URL
  private let width: Int
  private let height: Int
  private let interval: TimeInterval
  private var timer: Timer?

  init(
    view: NSView,
    outputPath: String,
    width: Int,
    height: Int,
    intervalMillis: UInt64
  ) {
    self.view = view
    self.outputURL = URL(fileURLWithPath: outputPath)
    self.width = width
    self.height = height
    self.interval = max(0.05, Double(intervalMillis) / 1000.0)
  }

  func start() throws {
    try writeFrame()
    let timer = Timer.scheduledTimer(
      withTimeInterval: interval,
      repeats: true
    ) { [weak self] _ in
      do {
        try self?.writeFrame()
      } catch {
        FileHandle.standardError.write(
          Data("AppleVzRunner: display framebuffer export failed: \(error)\n".utf8))
      }
    }
    self.timer = timer
  }

  func stop() {
    timer?.invalidate()
    timer = nil
  }

  func writeFrame() throws {
    guard AppleVzDisplayLimits.supports(width: width, height: height) else {
      throw ExportError.invalidDimensions(width: width, height: height)
    }
    guard let view else {
      throw ExportError.viewUnavailable
    }
    let bounds = view.bounds
    guard let bitmap = view.bitmapImageRepForCachingDisplay(in: bounds) else {
      throw ExportError.bitmapCreationFailed
    }
    view.cacheDisplay(in: bounds, to: bitmap)
    guard let image = bitmap.cgImage else {
      throw ExportError.cgImageCreationFailed
    }

    let rgba = try Self.rgbaData(from: image, width: width, height: height)
    try FileManager.default.createDirectory(
      at: outputURL.deletingLastPathComponent(),
      withIntermediateDirectories: true
    )
    try rgba.write(to: outputURL, options: .atomic)
  }

  static func rgbaData(from image: CGImage, width: Int, height: Int) throws -> Data {
    guard AppleVzDisplayLimits.supports(width: width, height: height) else {
      throw ExportError.invalidDimensions(width: width, height: height)
    }
    let byteCount = width * height * 4
    var data = Data(count: byteCount)
    try data.withUnsafeMutableBytes { rawBuffer in
      guard let baseAddress = rawBuffer.baseAddress else {
        throw ExportError.contextCreationFailed
      }
      guard
        let context = CGContext(
          data: baseAddress,
          width: width,
          height: height,
          bitsPerComponent: 8,
          bytesPerRow: width * 4,
          space: CGColorSpaceCreateDeviceRGB(),
          bitmapInfo: CGBitmapInfo.byteOrder32Big.rawValue
            | CGImageAlphaInfo.premultipliedLast.rawValue
        )
      else {
        throw ExportError.contextCreationFailed
      }
      context.interpolationQuality = .none
      context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
    }
    return data
  }
}

private func runtimeResourcePolicyPath(bundlePath: String) -> String {
  URL(fileURLWithPath: bundlePath)
    .appendingPathComponent("metadata")
    .appendingPathComponent("runtime-resources.json")
    .path
}
#endif
