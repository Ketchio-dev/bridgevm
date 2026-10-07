#if canImport(Darwin)
import Darwin
import Foundation
import XCTest
@testable import AppleVzRunnerCore

final class RuntimeControlSocketCleanupTests: XCTestCase {
  func testCompletedStopRemovesOwnedSocketBeforeFixtureCleanup() throws {
    let path = runtimeControlSocketPath()
    let owned = makeServer(path: path)
    try owned.start()
    try stopRuntimeControlListener(owned)
    var info = stat()
    XCTAssertEqual(lstat(path, &info), -1)
    XCTAssertEqual(errno, ENOENT)
    try stopRuntimeControlListener(owned)
  }

  func testCompletedStopPreservesReplacementSocketAndItsListener() throws {
    let path = runtimeControlSocketPath()
    let owned = makeServer(path: path)
    try owned.start()
    defer { owned.stop() }
    XCTAssertEqual(unlink(path), 0)
    let replacement = socket(AF_UNIX, SOCK_STREAM, 0)
    guard replacement >= 0 else { throw RuntimeControlTeardownError.bind }
    defer { close(replacement) }
    try bindSocket(replacement, path: path)
    XCTAssertEqual(listen(replacement, 1), 0)
    var before = stat(); XCTAssertEqual(lstat(path, &before), 0)
    try stopRuntimeControlListener(owned)
    var after = stat(); XCTAssertEqual(lstat(path, &after), 0)
    XCTAssertEqual(after.st_mode & S_IFMT, mode_t(S_IFSOCK))
    XCTAssertEqual(after.st_dev, before.st_dev)
    XCTAssertEqual(after.st_ino, before.st_ino)
    let client = socket(AF_UNIX, SOCK_STREAM, 0)
    guard client >= 0 else { throw RuntimeControlTeardownError.bind }
    defer { close(client) }
    try withAddress(path) { XCTAssertEqual(connect(client, $0, $1), 0) }
  }

  func testStopCompletionWithoutSuccessfulStart() throws {
    let path = runtimeControlSocketPath()
    let server = makeServer(path: path)
    try stopRuntimeControlListener(server)
    try Data("keep".utf8).write(to: URL(fileURLWithPath: path))
    XCTAssertThrowsError(try server.start())
    try stopRuntimeControlListener(server)
    XCTAssertEqual(try String(contentsOfFile: path, encoding: .utf8), "keep")
  }

  private func makeServer(path: String) -> AppleVzDisplayRuntimeControlServer {
    AppleVzDisplayRuntimeControlServer(socketPath: path, statusProvider: {
      AppleVzDisplayRuntimeControlSnapshot(vmName: "test", state: "running",
        displayWidthInPixels: 1, displayHeightInPixels: 1, isStopping: false)
    }, stopHandler: {})
  }

  private func bindSocket(_ fd: Int32, path: String) throws {
    try withAddress(path) { pointer, count in
      guard Darwin.bind(fd, pointer, count) == 0 else { throw RuntimeControlTeardownError.bind }
    }
  }

  private func withAddress(_ path: String, _ body: (UnsafePointer<sockaddr>, socklen_t) throws -> Void) throws {
    var address = sockaddr_un(); address.sun_family = sa_family_t(AF_UNIX)
    let bytes = Array(path.utf8), capacity = MemoryLayout.size(ofValue: address.sun_path)
    guard bytes.count < capacity else { throw RuntimeControlTeardownError.bind }
    withUnsafeMutablePointer(to: &address.sun_path) { pointer in
      pointer.withMemoryRebound(to: CChar.self, capacity: capacity) { output in
        for (index, byte) in bytes.enumerated() { output[index] = CChar(bitPattern: byte) }
      }
    }
    try withUnsafePointer(to: &address) { pointer in
      try pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
        try body($0, socklen_t(MemoryLayout<sockaddr_un>.size))
      }
    }
  }
}
#endif
