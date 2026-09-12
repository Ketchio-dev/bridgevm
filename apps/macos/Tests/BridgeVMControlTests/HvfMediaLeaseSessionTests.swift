import XCTest
@testable import BridgeVMControl

final class HvfMediaLeaseSessionTests: XCTestCase {
    private func helper(_ body: String) throws -> URL {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString).resolvingSymlinksInPath()
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let executable = root.appendingPathComponent("helper")
        try Data(("#!/bin/sh\n" + body + "\n").utf8).write(to: executable)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)
        return executable
    }

    func testReadyThenExplicitReleaseSucceeds() throws {
        let executable = try helper("printf 'bridgevm-media-lease-v1\\n'; read command; test \"$command\" = release")
        let session = try HvfMediaLeaseSession(executable: executable, disk: "/unused/disk", vars: "/unused/vars")
        try session.finish()
        XCTAssertThrowsError(try session.finish())
    }

    func testMissingOrInvalidReadinessFails() throws {
        for body in ["exit 1", "printf 'wrong-native-lease-v1!!\\n'; read command"] {
            let executable = try helper(body)
            XCTAssertThrowsError(try HvfMediaLeaseSession(
                executable: executable, disk: "/unused/disk", vars: "/unused/vars", timeout: 0.3))
        }
    }

    func testUnresponsiveReadyIsBounded() throws {
        let executable = try helper("read command")
        let start = ProcessInfo.processInfo.systemUptime
        XCTAssertThrowsError(try HvfMediaLeaseSession(
            executable: executable, disk: "/unused/disk", vars: "/unused/vars", timeout: 0.1))
        XCTAssertLessThan(ProcessInfo.processInfo.systemUptime - start, 3)
    }

    func testReleaseFailureIsNotSuccess() throws {
        let executable = try helper("printf 'bridgevm-media-lease-v1\\n'; read command; exit 7")
        let session = try HvfMediaLeaseSession(executable: executable, disk: "/unused/disk", vars: "/unused/vars")
        XCTAssertThrowsError(try session.finish())
        session.abort()
    }
}
