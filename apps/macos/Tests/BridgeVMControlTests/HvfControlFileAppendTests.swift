import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfControlFileAppendTests: XCTestCase {
    func testConcurrentOwnedShutdownRetainsBothCompleteCommands() throws {
        let root = try directory()
        defer { try? FileManager.default.removeItem(at: root) }
        let control = root.appendingPathComponent("agent.ctl")
        try Data("existing\n".utf8).write(to: control)
        let owner = HvfOwnedGuestShutdown(path: control.path)
        defer { owner.close() }
        let finished = DispatchSemaphore(value: 0)
        var writes = 0
        try HvfControlFileAppend.append(Data("KEYINPUT fixture enter\n".utf8), to: control) { descriptor, bytes, count in
            writes += 1
            // The GUI writer is ready. Let the real background owner append shutdown before its write.
            owner.request { delivered in
                XCTAssertTrue(delivered)
                finished.signal()
            }
            XCTAssertEqual(finished.wait(timeout: .now() + 10), .success)
            return Darwin.write(descriptor, bytes, count)
        }
        XCTAssertEqual(writes, 1)
        XCTAssertEqual(try String(contentsOf: control, encoding: .utf8),
                       "existing\nshutdown.exe /p /f\nKEYINPUT fixture enter\n")
    }

    func testPartialOrFailedWriteIsRefusedWithoutRetry() throws {
        let root = try directory()
        defer { try? FileManager.default.removeItem(at: root) }
        let control = root.appendingPathComponent("agent.ctl")
        try Data("existing\n".utf8).write(to: control)
        var calls = 0
        XCTAssertThrowsError(try HvfControlFileAppend.append(Data("KEYINPUT fixture enter\n".utf8), to: control) {
            descriptor, bytes, _ in
            calls += 1
            return Darwin.write(descriptor, bytes, 3)
        })
        XCTAssertEqual(calls, 1)
        XCTAssertEqual(try String(contentsOf: control, encoding: .utf8), "existing\nKEY")
        XCTAssertThrowsError(try HvfControlFileAppend.append(Data("must not replay\n".utf8), to: control) { _, _, _ in
            calls += 1
            errno = EIO
            return -1
        })
        XCTAssertEqual(calls, 2)
        XCTAssertEqual(try String(contentsOf: control, encoding: .utf8), "existing\nKEY")
    }

    private func directory() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("control-append-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        return root
    }
}
