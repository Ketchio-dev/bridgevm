import Foundation
#if canImport(Darwin)
import Darwin
#endif

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
  private let listenerTeardown = DispatchGroup()
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
      guard lstat(socketPath, &info) == 0, (info.st_mode & S_IFMT) == S_IFSOCK else {
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

    let source = DispatchSource.makeReadSource(fileDescriptor: fd, queue: queue)
    source.setEventHandler { [weak self] in
      self?.acceptAvailableConnections(fd)
    }
    let ownedSocketIdentity = socketIdentity!
    listenerTeardown.enter()
    source.setCancelHandler { [socketPath, listenerTeardown] in
      defer { listenerTeardown.leave() }
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

  // Lifecycle calls are caller-serialized. Completion observes listener close
  // and conditional pathname cleanup, not draining accepted client requests.
  func stop(completion: (() -> Void)? = nil) {
    source?.cancel()
    source = nil
    if let completion { listenerTeardown.notify(queue: queue, execute: completion) }
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

  private func acceptAvailableConnections(_ fd: Int32) {
    let client = accept(fd, nil, nil)
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
