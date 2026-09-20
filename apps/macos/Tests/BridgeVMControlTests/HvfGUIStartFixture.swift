import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfGUIStartGate: @unchecked Sendable {
    private let lock = NSLock(), semaphore = DispatchSemaphore(value: 0)
    private var entered = false, expired = false
    var state: (entered: Bool, expired: Bool) {
        lock.lock(); defer { lock.unlock() }; return (entered, expired)
    }
    func wait() {
        lock.lock(); entered = true; lock.unlock()
        let result = semaphore.wait(timeout: .now() + 3)
        lock.lock(); expired = result == .timedOut; lock.unlock()
    }
    func release() { semaphore.signal() }
}

/// Only synthetic key bytes and this fixture's exact retained child are used.
final class HvfGUIStartEffects: VTPMStateKeyProviding, VTPMExistingStateKeyReading, @unchecked Sendable {
    struct Snapshot {
        var probes = 0, launches = 0, existingReads = 0
        var keyCreation: [Bool] = []
        var keyIDs: [String] = []
        var offMain: [Bool] = []
        var arguments: [String] = []
        var executable: String?
        var child: Process?
        var deliveryPipeClosed = false
    }
    let root: URL
    let probeGate: HvfGUIStartGate?, keyGate: HvfGUIStartGate?, launchGate: HvfGUIStartGate?
    let foundExisting: Bool, rejectLaunch: Bool, actualRunner: Bool, failKeyDelivery: Bool
    private let lock = NSLock()
    private var stored = Snapshot()
    var snapshot: Snapshot { lock.lock(); defer { lock.unlock() }; return stored }
    init(root: URL, foundExisting: Bool = false, rejectLaunch: Bool = false, actualRunner: Bool = false, failKeyDelivery: Bool = false,
         probeGate: HvfGUIStartGate? = nil, keyGate: HvfGUIStartGate? = nil, launchGate: HvfGUIStartGate? = nil) {
        self.root = root; self.foundExisting = foundExisting; self.rejectLaunch = rejectLaunch; self.actualRunner = actualRunner; self.failKeyDelivery = failKeyDelivery
        self.probeGate = probeGate; self.keyGate = keyGate; self.launchGate = launchGate
    }
    func probe(_ path: String) -> Bool {
        lock.lock(); stored.probes += 1; stored.offMain.append(!Thread.isMainThread); lock.unlock()
        probeGate?.wait(); return foundExisting
    }
    func stateKey(for stableVMID: String, allowCreation: Bool) throws -> Data {
        lock.lock(); stored.keyCreation.append(allowCreation); stored.keyIDs.append(stableVMID); stored.offMain.append(!Thread.isMainThread); lock.unlock()
        keyGate?.wait(); return Data(repeating: 0x42, count: 32)
    }
    func existingStateKey(for stableVMID: String) throws -> Data {
        lock.lock(); stored.existingReads += 1; lock.unlock()
        throw CocoaError(.fileReadNoPermission)
    }
    func launch(_ process: Process) throws {
        lock.lock(); stored.launches += 1; stored.offMain.append(!Thread.isMainThread)
        stored.arguments = process.arguments ?? []; stored.executable = process.executableURL?.path; lock.unlock()
        if rejectLaunch { throw CocoaError(.executableNotLoadable) }
        if actualRunner { process.environment?["TMPDIR"] = "/tmp" }
        else {
            process.executableURL = URL(fileURLWithPath: "/bin/sh")
            process.arguments = ["-c", "trap '' TERM; printf ready > \"$1\"; n=0; while [ ! -e \"$2\" ] && [ $n -lt 400 ]; do /bin/sleep 0.01; n=$((n+1)); done; [ -e \"$2\" ] || exit 98; exit 17",
                "gui-start-child", root.appendingPathComponent("shell-ready").path, root.appendingPathComponent("fixture-release").path]
            process.standardOutput = FileHandle.nullDevice; process.standardError = FileHandle.nullDevice
        }
        try process.run()
        lock.lock(); stored.child = process; lock.unlock()
        if !actualRunner {
            let deadline = ProcessInfo.processInfo.systemUptime + 2
            while !FileManager.default.fileExists(atPath: root.appendingPathComponent("shell-ready").path),
                  process.isRunning, ProcessInfo.processInfo.systemUptime < deadline { Thread.sleep(forTimeInterval: 0.005) }
        }
        if failKeyDelivery, let pipe = process.standardInput as? Pipe {
            do {
                try pipe.fileHandleForWriting.close()
                lock.lock(); stored.deliveryPipeClosed = true; lock.unlock()
            } catch { /* The snapshot assertion reports failure without throwing after spawning. */ }
        }
        launchGate?.wait()
    }
    func releaseGates() { probeGate?.release(); keyGate?.release(); launchGate?.release() }
}

@MainActor
final class HvfGUIStartFixture {
    private static var unresolved: [HvfGUIStartFixture] = []
    let base: HvfOwnedRuntimeFixture
    let effects: HvfGUIStartEffects
    let session: HvfEngineSession
    let nonce = UUID().uuidString
    private var witnesses: [String: HvfOwnedRunnerExitWitness] = [:]
    var root: URL { base.root }
    init(encrypted: Bool = false, typed: Bool = false, actualRunner: Bool = false, foundExisting: Bool = false,
         rejectLaunch: Bool = false, failKeyDelivery: Bool = false, probeGate: HvfGUIStartGate? = nil, keyGate: HvfGUIStartGate? = nil,
         launchGate: HvfGUIStartGate? = nil) throws {
        base = try HvfOwnedRuntimeFixture(encrypted: encrypted)
        effects = HvfGUIStartEffects(root: base.root, foundExisting: foundExisting, rejectLaunch: rejectLaunch,
            actualRunner: actualRunner, failKeyDelivery: failKeyDelivery, probeGate: probeGate, keyGate: keyGate, launchGate: launchGate)
        let effects = effects
        session = HvfEngineSession(config: base.config, repoRoot: base.root,
            processLaunch: { try effects.launch($0) }, processIsRunning: { effects.probe($0) }, vtpmKeyProvider: effects)
        do {
            if typed || actualRunner {
                let runner = root.appendingPathComponent("target/release/hvf-runner")
                if actualRunner {
                    let path = try XCTUnwrap(ProcessInfo.processInfo.environment["BRIDGEVM_TEST_OWNED_RUNNER"])
                    try FileManager.default.copyItem(atPath: path, toPath: runner.path)
                    try installChild(role: "helper", at: root.appendingPathComponent("target/release/examples/hvf_gic_boot_probe"))
                    try installChild(role: "swtpm", at: URL(fileURLWithPath: base.config.swtpmBin))
                } else {
                    try Data("typed selection marker; never executed\n".utf8).write(to: runner)
                    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: runner.path)
                }
            }
        } catch {
            base.clean()
            throw error
        }
    }
    func start(configuration: HvfEngineConfig? = nil, policy: HvfRuntimeStartPolicy = .attachOrStart) throws -> HvfGUIStartOperation {
        guard case let .accepted(ticket) = session.requestGUIStart(configuration: configuration ?? session.config, policy: policy) else {
            throw CocoaError(.executableRuntimeMismatch)
        }
        return ticket
    }
    func wait(_ predicate: @MainActor () -> Bool, seconds: TimeInterval = 2) async throws {
        let deadline = ProcessInfo.processInfo.systemUptime + seconds
        while !predicate(), ProcessInfo.processInfo.systemUptime < deadline { try await Task.sleep(nanoseconds: 5_000_000) }
        guard predicate() else { XCTFail("Bounded GUI start fixture did not settle"); throw CocoaError(.executableRuntimeMismatch) }
    }
    private func installChild(role: String, at url: URL) throws {
        let source = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let template = try String(contentsOf: source.appendingPathComponent("tests/integration/native-owned-runner-child.py"), encoding: .utf8)
        let python = try NativeTestPython.executable()
        let settings = ["root": root.path, "role": role, "nonce": nonce, "control": base.config.ctlFilePath]
        let json = String(decoding: try JSONSerialization.data(withJSONObject: settings, options: [.sortedKeys, .withoutEscapingSlashes]), as: UTF8.self)
        let literal = String(decoding: try JSONSerialization.data(withJSONObject: json, options: [.fragmentsAllowed, .withoutEscapingSlashes]), as: UTF8.self)
        try Data(template.replacingOccurrences(of: "#!/usr/bin/env python3", with: "#!" + python.path)
            .replacingOccurrences(of: "__FIXTURE_CONFIG_LITERAL__", with: literal).utf8).write(to: url)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: url.path)
    }
    func observeChildren() throws {
        for role in ["helper", "swtpm"] where witnesses[role] == nil {
            let path = root.appendingPathComponent(role + ".started")
            guard FileManager.default.fileExists(atPath: path.path) else { continue }
            let row = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: path)) as? [String: Any])
            let pid = try XCTUnwrap(row["pid"] as? Int32)
            guard row["nonce"] as? String == nonce, row["role"] as? String == role,
                  row["parent"] as? Int32 == effects.snapshot.child?.processIdentifier else { throw CocoaError(.fileReadCorruptFile) }
            XCTAssertEqual(row["keyBytesRead"] as? Int, role == "swtpm" ? 32 : 0)
            witnesses[role] = try HvfOwnedRunnerExitWitness(pid: pid)
        }
    }
    var childrenObserved: Bool { witnesses.count == 2 }
    var childrenExited: Bool { childrenObserved && witnesses.values.allSatisfy { $0.observe() } }
    private var confirmedChildrenExited: Bool {
        guard let complete = session.ownedController?.ledger.complete else { return false }
        let expected = [("helper", complete.helper), ("swtpm", complete.swtpm)]
        return expected.allSatisfy { role, summary in HvfOwnedRunnerReapedEvidence.confirmed(summary, role: role, witness: witnesses[role]) }
    }
    func observeRuntime() {
        if session.guiStartOperation?.invalidationReason != nil {
            if let controller = session.ownedController { controller.tick() }
            else if let child = effects.snapshot.child, session.process === child, !child.isRunning { session.markStopped() }
        } else { session.poll() }
    }
    func clean() async {
        effects.releaseGates(); try? Data().write(to: root.appendingPathComponent("fixture-release"))
        let deadline = ProcessInfo.processInfo.systemUptime + 16
        var safe = false
        repeat {
            if effects.actualRunner { try? observeChildren() }
            observeRuntime()
            let child = effects.snapshot.child
            let exited = child == nil || (child?.isRunning != true && (!effects.actualRunner || confirmedChildrenExited))
            safe = !session.hasPendingGUIStart && !session.mayHaveOwnedWork && exited
            if safe { break }; try? await Task.sleep(nanoseconds: 10_000_000)
        } while ProcessInfo.processInfo.systemUptime < deadline
        XCTAssertTrue(safe, "Retaining unresolved GUI worker/child at \(root.path)")
        if safe { base.clean() }
        else if !Self.unresolved.contains(where: { $0 === self }) { Self.unresolved.append(self) }
    }
}
