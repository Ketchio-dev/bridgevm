import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

/// Thread-safe synthetic effects used by the real detached start worker.
final class HvfOwnedStartFixtureEffects: VTPMStateKeyProviding, VTPMExistingStateKeyReading, @unchecked Sendable {
    private let lock = NSLock()
    private var retained: Process?
    private var launchCount = 0, creationCount = 0
    private var keyIDs: [String] = []
    var snapshot: (child: Process?, launches: Int, creations: Int, keyIDs: [String]) {
        lock.lock(); defer { lock.unlock() }; return (retained, launchCount, creationCount, keyIDs)
    }
    func launch(_ process: Process) throws {
        lock.lock(); retained = process; launchCount += 1; lock.unlock()
        process.environment?["TMPDIR"] = "/tmp"
        try process.run()
    }
    func stateKey(for stableVMID: String, allowCreation: Bool) throws -> Data {
        lock.lock(); creationCount += 1; lock.unlock()
        throw CocoaError(.fileWriteNoPermission)
    }
    func existingStateKey(for stableVMID: String) throws -> Data {
        lock.lock(); keyIDs.append(stableVMID); lock.unlock()
        return Data(repeating: 0x42, count: 32) // Synthetic child protocol bytes only; never a host key.
    }
}

@MainActor
final class HvfOwnedStartRunnerFixture {
    private static var environmentInUse = false
    private static var unresolved: [HvfOwnedStartRunnerFixture] = []
    let root: URL
    let libraryRoot: URL
    let config: VMConfig
    let effects = HvfOwnedStartFixtureEffects()
    let nonce = UUID().uuidString
    private let previousOverride: String?
    private(set) var mapped: HvfEngineConfig!
    private(set) var session: HvfEngineSession?
    private(set) var witnesses: [String: HvfOwnedRunnerExitWitness] = [:]
    private(set) var childIDs: [String: Int32] = [:]

    init() throws {
        guard !Self.environmentInUse else { throw CocoaError(.fileLocking) }
        root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath().appendingPathComponent("owned-start-" + UUID().uuidString)
        libraryRoot = root.appendingPathComponent("library")
        config = VMConfig(id: "owned-start", name: "owned-start", displayName: "Owned start fixture",
            backendKind: "hvf-engine", bootMode: "windows-hvf", bundlePath: libraryRoot.appendingPathComponent("owned-start/bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
            guestName: "fixture", displayWidth: 1280, displayHeight: 720, installPending: false,
            memMiB: 1024, cpuCount: 1, networkEnabled: false, experimental3DAllowed: false)
        previousOverride = getenv("BRIDGEVM_SWTPM_BIN").map { String(cString: $0) }
        Self.environmentInUse = true
        setenv("BRIDGEVM_SWTPM_BIN", root.appendingPathComponent("swtpm-fixture").path, 1)
        do {
            mapped = try XCTUnwrap(HvfEngineConfig.libraryVM(config, rootURL: libraryRoot))
            let runnerPath = try XCTUnwrap(ProcessInfo.processInfo.environment["BRIDGEVM_TEST_OWNED_RUNNER"])
            guard runnerPath.hasPrefix("/"), FileManager.default.isExecutableFile(atPath: runnerPath) else { throw CocoaError(.executableNotLoadable) }
            for path in [mapped.targetDiskPath, mapped.uefiVarsPath, root.appendingPathComponent("target/release/hvf-runner").path,
                         root.appendingPathComponent("target/release/examples/hvf_gic_boot_probe").path,
                         root.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh").path] {
                try FileManager.default.createDirectory(at: URL(fileURLWithPath: path).deletingLastPathComponent(), withIntermediateDirectories: true)
            }
            try Data([0]).write(to: URL(fileURLWithPath: mapped.targetDiskPath))
            FileManager.default.createFile(atPath: mapped.uefiVarsPath, contents: nil)
            let vars = try FileHandle(forWritingTo: URL(fileURLWithPath: mapped.uefiVarsPath))
            try vars.truncate(atOffset: 64 * 1024 * 1024); try vars.close()
            try FileManager.default.copyItem(atPath: runnerPath, toPath: root.appendingPathComponent("target/release/hvf-runner").path)
            let wrapper = root.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh")
            try Data("fixture readiness marker; never executed\n".utf8).write(to: wrapper)
            try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: wrapper.path)
            try installChild(role: "helper", at: root.appendingPathComponent("target/release/examples/hvf_gic_boot_probe"))
            try installChild(role: "swtpm", at: URL(fileURLWithPath: mapped.swtpmBin))
            guard VMLibrary.save(config, rootURL: libraryRoot) else { throw CocoaError(.fileWriteUnknown) }
        } catch { restoreEnvironment(); try? FileManager.default.removeItem(at: root); throw error }
    }

    func makeSession(_ configuration: HvfEngineConfig) -> HvfEngineSession {
        XCTAssertEqual(configuration, mapped, "Fixture must use the actual saved-library launch mapping")
        XCTAssertNil(session, "Only one retained session may be created for this saved VM")
        let value = HvfEngineSession(config: configuration, repoRoot: root,
            processLaunch: { [effects] in try effects.launch($0) }, processIsRunning: { _ in false }, vtpmKeyProvider: effects)
        session = value
        return value
    }

    private func installChild(role: String, at url: URL) throws {
        let source = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let template = try String(contentsOf: source.appendingPathComponent("tests/integration/native-owned-runner-child.py"), encoding: .utf8)
        let python = try NativeTestPython.executable()
        let settings = ["root": root.path, "role": role, "nonce": nonce, "control": mapped.ctlFilePath]
        let json = String(decoding: try JSONSerialization.data(withJSONObject: settings, options: [.sortedKeys, .withoutEscapingSlashes]), as: UTF8.self)
        let literal = String(decoding: try JSONSerialization.data(withJSONObject: json, options: [.fragmentsAllowed, .withoutEscapingSlashes]), as: UTF8.self)
        let body = template.replacingOccurrences(of: "#!/usr/bin/env python3", with: "#!" + python.path)
            .replacingOccurrences(of: "__FIXTURE_CONFIG_LITERAL__", with: literal)
        try Data(body.utf8).write(to: url)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: url.path)
    }

    func adoptChildren() throws {
        for role in ["helper", "swtpm"] where witnesses[role] == nil {
            let path = root.appendingPathComponent(role + ".started")
            guard FileManager.default.fileExists(atPath: path.path) else { continue }
            let row = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: path)) as? [String: Any])
            let pid = try XCTUnwrap(row["pid"] as? Int32)
            guard row["role"] as? String == role, row["nonce"] as? String == nonce,
                  row["parent"] as? Int32 == effects.snapshot.child?.processIdentifier else { throw CocoaError(.fileReadCorruptFile) }
            XCTAssertEqual(row["keyBytesRead"] as? Int, role == "swtpm" ? 32 : 0)
            witnesses[role] = try HvfOwnedRunnerExitWitness(pid: pid); childIDs[role] = pid
        }
    }

    func awaitChildren() async throws {
        let deadline = ProcessInfo.processInfo.systemUptime + 5
        while witnesses.count < 2, ProcessInfo.processInfo.systemUptime < deadline {
            try adoptChildren(); session?.poll()
            if witnesses.count < 2 { try await Task.sleep(nanoseconds: 10_000_000) }
        }
        guard witnesses.count == 2 else { throw CocoaError(.fileReadUnknown) }
    }

    func clean() async {
        try? Data().write(to: root.appendingPathComponent("fixture-release"))
        let deadline = ProcessInfo.processInfo.systemUptime + 16
        var safe = false
        repeat {
            try? adoptChildren(); session?.poll()
            let child = effects.snapshot.child
            let noWorker = session?.hasPendingOwnedStart != true
            let noRuntime = session?.mayHaveOwnedWork != true
            let exited = child == nil || (child?.isRunning != true && witnesses.count == 2 && witnesses.values.allSatisfy { $0.observe() })
            safe = noWorker && noRuntime && exited
            if safe { break }
            try? await Task.sleep(nanoseconds: 10_000_000)
        } while ProcessInfo.processInfo.systemUptime < deadline
        XCTAssertTrue(safe, "Retaining unresolved owned start fixture at \(root.path)")
        if safe { restoreEnvironment(); try? FileManager.default.removeItem(at: root) }
        else if !Self.unresolved.contains(where: { $0 === self }) { Self.unresolved.append(self) }
    }

    private func restoreEnvironment() {
        if let previousOverride { setenv("BRIDGEVM_SWTPM_BIN", previousOverride, 1) } else { unsetenv("BRIDGEVM_SWTPM_BIN") }
        Self.environmentInUse = false
    }
}
