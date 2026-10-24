import XCTest
@testable import BridgeVMProductE2E

/// The check never throws: it boots, enables, restarts and reads within bounded waits, records each
/// failure as one fixed reason, stops the VM, and puts the journey's final run log back unchanged.
final class T19BitLockerCheckTests: XCTestCase {
    private typealias Result = T19BitLockerGuestResultTests
    private let nonce = T19BitLockerGuestResultTests.nonce
    private let short = T19BitLockerTimeouts(boot: 0.6, asset: 0.4, launch: 0.4, result: 0.4, restart: 0.4, shutdown: 0.6, productStop: 0.4)
    private let journeyLog = Data("BVAGENT READY host=BRIDGEVM t=1\njourney final log\n".utf8)
    private var root: URL!
    private var bundle: URL { root.appendingPathComponent("bundle") }
    private var runLog: URL { bundle.appendingPathComponent("logs/hvf/run.log") }

    override func setUpWithError() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent("t19-bitlocker-check-\(UUID().uuidString)")
        let assets = root.appendingPathComponent("BridgeVM.app/Contents/Resources/scripts/win-assets")
        for directory in [root.appendingPathComponent("share"), runLog.deletingLastPathComponent(), assets,
                          bundle.appendingPathComponent("metadata/product-e2e")] {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        }
        for name in T19BitLockerGuestAssets.names { try Data("asset\r\n".utf8).write(to: assets.appendingPathComponent(name)) }
        try journeyLog.write(to: runLog)
    }

    override func tearDownWithError() throws { try? FileManager.default.removeItem(at: root) }

    private func run(_ guest: FakeGuest) -> (evidence: T19BitLockerEvidence, seconds: TimeInterval) {
        let started = Date()
        let check = T19BitLockerCheck(request: CheckRequest(root: root, nonce: nonce), appBundlePath: root.appendingPathComponent("BridgeVM.app").path,
                                      ui: guest, timeouts: short, vmGone: { _ in guest.stopped })
        let evidence = check.run()
        return (evidence, Date().timeIntervalSince(started))
    }

    private func assertJourneyLogRestored(_ name: String = "") throws {
        XCTAssertEqual(try Data(contentsOf: runLog), journeyLog, name)
        XCTAssertFalse(FileManager.default.fileExists(atPath: bundle.appendingPathComponent("metadata/product-e2e/bitlocker-held-run.log").path), name)
    }

    func testAnUnlockedRestartIsRecordedAndTheJourneyLogIsPutBack() throws {
        let guest = FakeGuest(root: root, nonce: nonce)
        let (evidence, _) = run(guest)
        XCTAssertEqual(guest.events, ["start", "launch BitLockerEnable", "shutdown.exe /r /t 0 /f", "launch BitLockerStatus", "shutdown.exe /s /t 0 /f"])
        XCTAssertEqual(evidence.outcome, .unlocked); XCTAssertNil(evidence.reason)
        XCTAssertEqual(evidence.enable, T19BitLockerBootRecord(ready: true, reason: nil, state: Result.enabled))
        var unlocked = Result.enabled; unlocked.percent = 37
        XCTAssertEqual(evidence.unlock, T19BitLockerBootRecord(ready: true, reason: nil, state: unlocked))
        XCTAssertEqual(evidence.restartObserved, true); XCTAssertEqual(evidence.powerCycleAfterRestart, true)
        XCTAssertEqual(evidence.equal(\.pcr0SHA256), true); XCTAssertEqual(evidence.stop, .guestShutdown)
        try assertJourneyLogRestored()
        let archived = try String(contentsOf: bundle.appendingPathComponent("metadata/product-e2e/bitlocker-run.log"), encoding: .utf8)
        XCTAssertTrue(archived.contains(HvfStopLine.systemResetPrefix) && archived.contains(T19BitLockerRunLog.powerCycle))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.appendingPathComponent("share").path)
            .filter { $0.hasPrefix("bv-") }.sorted(), T19BitLockerGuestAssets.names.sorted())
    }

    func testAGuestRefusalStopsTheCheckBeforeAnyRestart() throws {
        let guest = FakeGuest(root: root, nonce: nonce)
        guest.enableBody = Result.body(outcome: "edition-unsupported", edition: "home", volume: "unknown", protectors: "unknown", profile: "unknown")
        let (evidence, _) = run(guest)
        XCTAssertEqual(guest.events, ["start", "launch BitLockerEnable", "shutdown.exe /s /t 0 /f"])
        XCTAssertEqual(evidence.outcome, .refused); XCTAssertEqual(evidence.reason, .editionUnsupported)
        XCTAssertEqual(evidence.enable.reason, .editionUnsupported); XCTAssertEqual(evidence.enable.state?.edition, "home")
        XCTAssertEqual(evidence.unlock, T19BitLockerBootRecord()); XCTAssertNil(evidence.restartObserved)
        XCTAssertEqual(evidence.stop, .guestShutdown)
        try assertJourneyLogRestored()
    }

    func testAMissingReadyAfterTheRestartIsTheRecoverySignalAndTheProductStopsTheVM() throws {
        for (stops, stop) in [(true, T19BitLockerStop.productStop), (false, .unconfirmed)] {
            try? FileManager.default.removeItem(at: bundle.appendingPathComponent("metadata/product-e2e/bitlocker-run.log"))
            let guest = FakeGuest(root: root, nonce: nonce); guest.unlocks = false; guest.stopWorks = stops
            let (evidence, seconds) = run(guest)
            XCTAssertEqual(guest.events, ["start", "launch BitLockerEnable", "shutdown.exe /r /t 0 /f", "stop"])
            XCTAssertEqual(evidence.outcome, .recoverySuspected); XCTAssertEqual(evidence.reason, .unlockBootNotReady)
            XCTAssertEqual(evidence.unlock, T19BitLockerBootRecord(ready: false, reason: .unlockBootNotReady, state: nil))
            XCTAssertEqual(evidence.restartObserved, true); XCTAssertEqual(evidence.powerCycleAfterRestart, true)
            XCTAssertEqual(evidence.stop, stop); XCTAssertLessThan(seconds, 6)
            try assertJourneyLogRestored("\(stop)")
        }
    }

    func testHostFailuresAreInconclusiveFixedReasonsWithinTheirBounds() throws {
        let cases: [(String, (FakeGuest) -> Void, T19BitLockerReason?, [String], T19BitLockerStop)] = [
            ("no READY", { $0.boots = false }, .bootNotReady, ["start", "stop"], .productStop),
            ("assets unsynchronized", { $0.syncs = false }, .assetMissing, ["start", "shutdown.exe /s /t 0 /f"], .guestShutdown),
            ("launch unanswered", { $0.replies = false }, .launchFailed, ["start", "launch BitLockerEnable", "shutdown.exe /s /t 0 /f"], .guestShutdown),
            ("no result", { $0.enableBody = nil }, .resultMissing, ["start", "launch BitLockerEnable", "shutdown.exe /s /t 0 /f"], .guestShutdown),
            ("malformed result", { $0.enableBody = Result.body(protectors: "1,1") }, .resultMalformed,
             ["start", "launch BitLockerEnable", "shutdown.exe /s /t 0 /f"], .guestShutdown),
            ("restart not observed", { $0.restarts = false }, .restartNotObserved,
             ["start", "launch BitLockerEnable", "shutdown.exe /r /t 0 /f", "shutdown.exe /s /t 0 /f"], .guestShutdown),
            ("assets not pushed again after the restart", { $0.resyncs = false }, .assetMissing,
             ["start", "launch BitLockerEnable", "shutdown.exe /r /t 0 /f", "shutdown.exe /s /t 0 /f"], .guestShutdown),
            ("no new generation after the reset", { $0.unlocks = false; $0.powerCycles = false }, .unlockBootNotReady,
             ["start", "launch BitLockerEnable", "shutdown.exe /r /t 0 /f", "stop"], .productStop),
            ("decrypted at the unlock boot", { $0.statusBody = Result.body(phase: "status", volume: "0,0,0,0") }, .unlockStateUnexpected,
             ["start", "launch BitLockerEnable", "shutdown.exe /r /t 0 /f", "launch BitLockerStatus", "shutdown.exe /s /t 0 /f"], .guestShutdown),
            ("a second protector at the unlock boot", { $0.statusBody = Result.body(phase: "status", protectors: "1,3") }, .unlockStateUnexpected,
             ["start", "launch BitLockerEnable", "shutdown.exe /r /t 0 /f", "launch BitLockerStatus", "shutdown.exe /s /t 0 /f"], .guestShutdown),
            ("status query failed", { $0.statusBody = Result.body(phase: "status", outcome: "query-failed", volume: "unknown", protectors: "unknown", profile: "unknown") },
             .queryFailed, ["start", "launch BitLockerEnable", "shutdown.exe /r /t 0 /f", "launch BitLockerStatus", "shutdown.exe /s /t 0 /f"], .guestShutdown),
            ("no SYSTEM_OFF", { $0.shutsDown = false }, .none,
             ["start", "launch BitLockerEnable", "shutdown.exe /r /t 0 /f", "launch BitLockerStatus", "shutdown.exe /s /t 0 /f", "stop"], .productStop),
        ]
        for (name, configure, reason, events, stop) in cases {
            try? FileManager.default.removeItem(at: bundle.appendingPathComponent("metadata/product-e2e/bitlocker-run.log"))
            for file in try FileManager.default.contentsOfDirectory(atPath: root.appendingPathComponent("share").path) {
                try FileManager.default.removeItem(at: root.appendingPathComponent("share/\(file)"))
            }
            let guest = FakeGuest(root: root, nonce: nonce); configure(guest)
            let (evidence, seconds) = run(guest)
            XCTAssertEqual(guest.events, events, name)
            XCTAssertEqual(evidence.reason, reason, name)
            XCTAssertEqual(evidence.outcome, reason == nil ? .unlocked : .inconclusive, name)
            XCTAssertEqual(evidence.stop, stop, name)
            XCTAssertLessThan(seconds, 8, name)
            try assertJourneyLogRestored(name)
        }
    }

    func testWithoutTheJourneyLogOrAStartTheCheckBootsNothing() throws {
        try FileManager.default.removeItem(at: runLog)
        let guest = FakeGuest(root: root, nonce: nonce)
        let (missing, _) = run(guest)
        XCTAssertEqual(guest.events, []); XCTAssertEqual(missing.outcome, .notRun); XCTAssertEqual(missing.reason, .logHoldFailed)
        XCTAssertEqual(missing.stop, .notStarted)
        try journeyLog.write(to: runLog)
        let refused = FakeGuest(root: root, nonce: nonce); refused.startWorks = false; refused.stopped = true
        let (unstarted, _) = run(refused)
        XCTAssertEqual(refused.events, ["start"]); XCTAssertEqual(unstarted.reason, .bootNotReady)
        XCTAssertEqual(unstarted.enable, T19BitLockerBootRecord(ready: false, reason: .bootNotReady, state: nil))
        XCTAssertEqual(unstarted.stop, .notStarted)
        try assertJourneyLogRestored()
    }
}

private struct CheckRequest: T17JourneyRequest {
    let root: URL; let nonce: String
    var jobID: String { "t19-bitlocker" }; var commit: String { String(repeating: "0", count: 40) }
    var lane: Int { 1 }; var vmSlug: String { "t19-bitlocker" }
    var libraryRootPath: String { root.appendingPathComponent("library").path }
    var sharePath: String { root.appendingPathComponent("share").path }
    var diskPath: String { root.appendingPathComponent("bundle/disks/hvf-target.raw").path }
    var varsPath: String { root.appendingPathComponent("bundle/metadata/hvf-vars.fd").path }
    var snapshotPath: String { root.appendingPathComponent("bundle/metadata/snapshots/latest.snapshot").path }
    var guestEvidencePath: String { root.appendingPathComponent("bundle/metadata/product-e2e-guest-evidence.json").path }
    var bundlePath: String { root.appendingPathComponent("bundle").path }
}

/// Models the product and the guest: Start writes a new run log, launches reply at once, a restart is
/// an exit-42 respawn, and a shutdown prints the final report.
private final class FakeGuest: T17UIControlling {
    var events: [String] = []
    var startWorks = true, boots = true, syncs = true, replies = true, restarts = true, unlocks = true, shutsDown = true, stopWorks = true
    var powerCycles = true, resyncs = true
    var stopped = false
    var enableBody: String? = T19BitLockerGuestResultTests.body()
    var statusBody: String? = T19BitLockerGuestResultTests.body(phase: "status", volume: "2,0,0,37")
    let root: URL, nonce: String
    private var command = ""
    init(root: URL, nonce: String) { self.root = root; self.nonce = nonce }
    private var prefix: String { String(nonce.prefix(12)) }
    private var runLog: URL { root.appendingPathComponent("bundle/logs/hvf/run.log") }

    private func append(_ text: String) throws {
        let handle = try FileHandle(forWritingTo: runLog); defer { try? handle.close() }
        try handle.seekToEnd(); try handle.write(contentsOf: Data(text.utf8))
    }

    func press(_ identifier: String, timeout: TimeInterval) throws {
        switch identifier {
        case "bridgevm.windows.runtime.start":
            events.append("start")
            guard startWorks else { throw T17Blocker(code: "ui-element-missing", detail: "start") }
            let synced = syncs ? T19BitLockerCheck.assets.map { "BVAGENT SHARE host->guest \($0) bytes=1 t=1\n" }.joined() : ""
            try Data(((boots ? "BVAGENT READY host=BRIDGEVM t=1\n" : "UEFI firmware\n") + synced).utf8).write(to: runLog)
        case "bridgevm.windows.runtime.stop":
            events.append("stop"); if stopWorks { stopped = true }
        case T17RuntimeControlInput.send:
            try deliver(command)
        default: break
        }
    }

    private func deliver(_ command: String) throws {
        guard command.contains(" -Action ") else {
            events.append(command)
            if command == "shutdown.exe /r /t 0 /f", restarts {
                let synced = resyncs ? T19BitLockerCheck.assets.map { "BVAGENT SHARE host->guest \($0) bytes=1 t=2\n" }.joined() : ""
                try append("\(HvfStopLine.systemResetPrefix)exiting for process recreation (exit 42)\n"
                           + (powerCycles ? "\(T19BitLockerRunLog.powerCycle)\n" : "")
                           + (unlocks ? "BVAGENT READY host=BRIDGEVM t=2\n" + synced : "BdsDxe: loading Boot0001\n"))
            } else if command == "shutdown.exe /s /t 0 /f", shutsDown {
                let serial = "UEFI firmware\r\n"
                try append("=== EDK2 boot probe (with Apple hv_gic) ===\n\(HvfStopLine.systemOff)\nexits: 1\n"
                           + "serial raw bytes: \(serial.utf8.count) output bytes: \(serial.utf8.count)\n--- serial (tail) ---\n\(serial)\n--- end ---\n"
                           + T17FirstRunHostRecordsTests.teardown)
            }
            return
        }
        let action = command.components(separatedBy: " -Action ")[1].components(separatedBy: " ")[0]
        events.append("launch \(action)")
        guard replies else { return }
        try append("BVAGENT CMD \(command) exit=0\nT19-BITLOCKER-LAUNCHED-\(action)-\(prefix) pid=7\nBVAGENT END \(command)\n")
        let phase = action == "BitLockerEnable" ? T19BitLockerPhase.enable : .status
        if let body = phase == .enable ? enableBody : statusBody {
            try Data(body.utf8).write(to: root.appendingPathComponent("share/\(T19BitLockerGuestResult.name(phase: phase, prefix: prefix))"))
        }
    }

    func fill(_ value: String, identifier: String, timeout: TimeInterval) throws { if identifier == T17RuntimeControlInput.input { command = value } }
    func waitFor(_ identifier: String, timeout: TimeInterval) throws {}
    func expand(_ identifier: String, timeout: TimeInterval) throws {}
    func setText(_ value: String, identifier: String, timeout: TimeInterval) throws {}
    func setToggle(_ enabled: Bool, identifier: String, timeout: TimeInterval) throws {}
    func choose(path: String, from identifier: String, timeout: TimeInterval) throws {}
    func text(_ identifier: String, timeout: TimeInterval) throws -> String { "" }
    func optionalTexts(_ identifiers: Set<String>) throws -> [String: String] { [:] }
    func clickDisplaySurface(at spot: CGPoint, timeout: TimeInterval) throws {}
    func textSnapshot() -> [String] { [] }
}
