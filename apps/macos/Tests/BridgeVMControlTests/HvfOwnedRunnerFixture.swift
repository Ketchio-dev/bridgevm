import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfOwnedRunnerFixture {
    let base: HvfOwnedRuntimeFixture
    let nonce = UUID().uuidString
    private(set) var session: HvfEngineSession!
    private(set) var witnesses: [String: HvfOwnedRunnerExitWitness] = [:]
    private var childIDs: [String: Int32] = [:]

    init() throws {
        guard let path = ProcessInfo.processInfo.environment["BRIDGEVM_TEST_OWNED_RUNNER"],
              path.hasPrefix("/"), FileManager.default.isExecutableFile(atPath: path) else {
            throw NSError(domain: "OwnedRunnerFixture", code: 1,
                userInfo: [NSLocalizedDescriptionKey: "Build and pass the required actual runner fixture helper"])
        }
        base = try HvfOwnedRuntimeFixture(encrypted: true)
        do {
            let runner = base.root.appendingPathComponent("target/release/hvf-runner")
            try FileManager.default.copyItem(atPath: path, toPath: runner.path)
            try installChild(role: "helper", at: base.root.appendingPathComponent("target/release/examples/hvf_gic_boot_probe"))
            try installChild(role: "swtpm", at: URL(fileURLWithPath: base.config.swtpmBin))
            session = HvfEngineSession(config: base.config, repoRoot: base.root,
                processLaunch: { [base] process in
                    base.launches += 1; base.child = process
                    process.environment?["TMPDIR"] = "/tmp"
                    try process.run()
                }, processIsRunning: { [base] _ in base.lookups += 1; return false },
                vtpmKeyProvider: base.keys)
        } catch { base.clean(); throw error }
    }

    private func installChild(role: String, at url: URL) throws {
        let sourceRoot = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let template = try String(contentsOf: sourceRoot.appendingPathComponent("tests/integration/native-owned-runner-child.py"), encoding: .utf8)
        let python = try NativeTestPython.executable()
        let config = ["root": base.root.path, "role": role, "nonce": nonce, "control": base.config.ctlFilePath]
        let json = String(decoding: try JSONSerialization.data(withJSONObject: config, options: [.sortedKeys, .withoutEscapingSlashes]), as: UTF8.self)
        let literal = String(decoding: try JSONSerialization.data(withJSONObject: json, options: [.fragmentsAllowed, .withoutEscapingSlashes]), as: UTF8.self)
        let body = template.replacingOccurrences(of: "#!/usr/bin/env python3", with: "#!" + python.path)
            .replacingOccurrences(of: "__FIXTURE_CONFIG_LITERAL__", with: literal)
        try Data(body.utf8).write(to: url)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: url.path)
    }

    func awaitChildren() async throws {
        let deadline = ProcessInfo.processInfo.systemUptime + 5
        while witnesses.count != 2, ProcessInfo.processInfo.systemUptime < deadline {
            try adoptChildren()
            session.poll()
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTAssertEqual(witnesses.count, 2, "Both expected owned children must identify themselves")
        guard witnesses.count == 2 else { throw CocoaError(.fileReadUnknown) }
        XCTAssertEqual(base.keys.requests, 1)
    }

    private func adoptChildren() throws {
        for role in ["helper", "swtpm"] where witnesses[role] == nil {
            let path = base.root.appendingPathComponent(role + ".started")
            guard FileManager.default.fileExists(atPath: path.path) else { continue }
            let raw = try JSONSerialization.jsonObject(with: Data(contentsOf: path))
            let row = try XCTUnwrap(raw as? [String: Any])
            let pid = try XCTUnwrap(row["pid"] as? Int32)
            guard row["role"] as? String == role, row["nonce"] as? String == nonce,
                  row["parent"] as? Int32 == base.child?.processIdentifier else {
                throw CocoaError(.fileReadCorruptFile)
            }
            XCTAssertEqual(row["keyBytesRead"] as? Int, role == "swtpm" ? 32 : 0)
            witnesses[role] = try HvfOwnedRunnerExitWitness(pid: pid)
            childIDs[role] = pid
        }
    }

    func waitForStop(_ operation: HvfOwnedStopOperation) async throws {
        let deadline = ProcessInfo.processInfo.systemUptime + 5
        while operation.observation.phase != .completed && operation.observation.phase != .unconfirmed,
              ProcessInfo.processInfo.systemUptime < deadline {
            session.poll()
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        let result = operation.observation
        XCTAssertTrue(result.isComplete, "Observed stop: \(result.phase), \(String(describing: result.failure))")
        let complete = try XCTUnwrap(result.supervisorComplete)
        let exit = try XCTUnwrap(result.runnerExit)
        XCTAssertEqual(exit.identity, operation.target)
        XCTAssertEqual(complete.helper.spawnedCount, 1)
        XCTAssertEqual(complete.helper.reapedCount, 1)
        XCTAssertEqual(complete.helper.last?.pid, childIDs["helper"])
        XCTAssertEqual(complete.swtpm.spawnedCount, 1)
        XCTAssertEqual(complete.swtpm.reapedCount, 1)
        XCTAssertEqual(complete.swtpm.last?.pid, childIDs["swtpm"])
        XCTAssertEqual(complete.mediaLeaseDisposition, "releasedAfterReap")
        XCTAssertEqual(complete.runtimeDirectoryDisposition, "removed")
        XCTAssertFalse(base.child?.isRunning == true)
        XCTAssertFalse(session.mayHaveOwnedWork)
        XCTAssertEqual(session.connectionState, .stopped)
    }

    func clean() async {
        if base.child == nil {
            // No launch was attempted; there can be no fixture-owned child to reap.
            try? FileManager.default.removeItem(at: base.root)
            return
        }
        try? Data().write(to: base.root.appendingPathComponent("fixture-release"))
        let deadline = ProcessInfo.processInfo.systemUptime + 16
        while ProcessInfo.processInfo.systemUptime < deadline {
            try? adoptChildren()
            session?.poll()
            if base.child?.isRunning != true && witnesses.count == 2 && witnesses.values.allSatisfy({ $0.observe() }) { break }
            try? await Task.sleep(nanoseconds: 10_000_000)
        }
        let safe = base.child?.isRunning != true && witnesses.count == 2 && witnesses.values.allSatisfy { $0.observe() }
        XCTAssertTrue(safe, "Preserving fixture because complete owned exit evidence is absent: \(base.root.path)")
        if safe { try? FileManager.default.removeItem(at: base.root) }
    }
}
