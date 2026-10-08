#if canImport(Darwin)
import Darwin
import Foundation
import XCTest
@testable import AppleVzRunnerCore

extension XCTestCase {
  func runtimeControlSocketPath() -> String {
    let path = "/tmp/bvm-rc-\(getpid())-\(UUID().uuidString.prefix(8)).sock"
    addTeardownBlock { try? FileManager.default.removeItem(atPath: path) }
    return path
  }
}

func stopRuntimeControlListener(_ server: AppleVzDisplayRuntimeControlServer) throws {
  let completed = DispatchSemaphore(value: 0)
  server.stop { completed.signal() }
  guard completed.wait(timeout: .now() + 2) == .success else {
    throw RuntimeControlTeardownError.deadline
  }
}

enum RuntimeControlTeardownError: Error { case deadline, bind }
#endif
