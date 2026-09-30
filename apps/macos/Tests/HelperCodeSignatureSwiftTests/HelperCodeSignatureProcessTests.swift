// sources: apps/macos/Sources/BridgeVMControl/HvfEngine/HelperCodeSignature.swift
import Foundation
import Testing

/// The codesign subprocess contract, driven through a stand-in tool. Each read
/// runs behind a watchdog, so a tool blocked on a full pipe fails a test
/// instead of hanging the suite.
@Suite("HelperCodeSignature process")
struct HelperCodeSignatureProcessTests {

    @Test("output on codesign's stdout cannot block the team read")
    func stdoutFloodDoesNotBlock() throws {
        let tool = try standInCodesign("""
            head -c 200000 /dev/zero | tr '\\0' x
            printf 'TeamIdentifier=ABCDE12345\\n' >&2
            """)
        defer { try? FileManager.default.removeItem(at: tool.deletingLastPathComponent()) }
        let read = try #require(teamRead(by: tool), "the read is blocked on a full stdout pipe")
        #expect(read == "ABCDE12345")
    }

    @Test("a failing codesign yields no team even when it printed one")
    func failingToolYieldsNoTeam() throws {
        let tool = try standInCodesign("printf 'TeamIdentifier=ABCDE12345\\n' >&2; exit 1")
        defer { try? FileManager.default.removeItem(at: tool.deletingLastPathComponent()) }
        let read = try #require(teamRead(by: tool), "the read did not return")
        #expect(read == nil)
    }

    private func standInCodesign(_ body: String) throws -> URL {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("codesign-stand-in-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        let tool = directory.appendingPathComponent("codesign")
        try Data("#!/bin/sh\n\(body)\n".utf8).write(to: tool)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: tool.path)
        return tool
    }

    /// nil when the read is still blocked at the deadline. The blocked child is
    /// left behind and dies of a broken pipe when this test process exits.
    private func teamRead(by tool: URL, seconds: Int = 15) -> String?? {
        let outcome = Outcome()
        let done = DispatchSemaphore(value: 0)
        Thread.detachNewThread {
            outcome.set(HelperCodeSignature.readTeamIdentifier(ofBinaryAt: "/bin/sh", codesign: tool))
            done.signal()
        }
        guard done.wait(timeout: .now() + .seconds(seconds)) == .success else { return nil }
        return .some(outcome.get())
    }

    private final class Outcome: @unchecked Sendable {
        private let lock = NSLock()
        private var value: String?

        func set(_ team: String?) {
            lock.lock(); defer { lock.unlock() }
            value = team
        }

        func get() -> String? {
            lock.lock(); defer { lock.unlock() }
            return value
        }
    }
}
