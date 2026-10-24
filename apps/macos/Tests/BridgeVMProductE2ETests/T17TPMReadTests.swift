import XCTest
@testable import BridgeVMProductE2E

/// One boot's TPM read never throws: every failure is a fixed reason within a bounded
/// wait, and the journey goes on to its next step whatever the guest answered.
final class T17TPMReadTests: XCTestCase {
    private let nonce = String(repeating: "5c", count: 32)
    private let short = T17TPMTimeouts(asset: 0.4, launch: 0.4, result: 0.4)
    private var root: URL!
    private var share: URL { root.appendingPathComponent("share") }
    private var runLog: URL { root.appendingPathComponent("bundle/logs/hvf/run.log") }
    private var command: String {
        "powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\\bridgevm-share\\bv-product-e2e-launch.ps1 -Action TpmBoot1 -Nonce \(nonce)"
    }

    override func setUpWithError() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent("t17-tpm-read-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: share, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: runLog.deletingLastPathComponent(), withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws { try? FileManager.default.removeItem(at: root) }

    private func synced(_ names: [String] = T17TPMBootReader.assets) -> String {
        names.map { "BVAGENT SHARE host->guest \($0) bytes=1 t=1\n" }.joined()
    }

    private func append(_ text: String) throws {
        if !FileManager.default.fileExists(atPath: runLog.path) { try Data().write(to: runLog) }
        let handle = try FileHandle(forWritingTo: runLog); defer { try? handle.close() }
        try handle.seekToEnd(); try handle.write(contentsOf: Data(text.utf8))
    }

    private func result(_ body: String, boot: T17TPMBoot = .first) throws {
        try Data(body.utf8).write(to: share.appendingPathComponent(T17TPMGuestResult.name(boot: boot, prefix: String(nonce.prefix(12)))))
    }

    private func reply(_ command: String, exit: Int = 0) throws {
        try append("BVAGENT CMD \(command) exit=\(exit)\nT17-LAUNCHED-TpmBoot1-5c5c5c5c5c5c pid=42\nBVAGENT END \(command)\n")
    }

    private func body(nonce: String? = nil) -> String {
        T17TPMGuestResultTests.body(nonce: nonce ?? self.nonce)
    }

    private func read(timeouts: T17TPMTimeouts? = nil, _ enter: (String) throws -> Void) -> (record: T17TPMBootRecord, seconds: TimeInterval) {
        let started = Date()
        let record = T17TPMBootReader(share: share, runLog: runLog, nonce: nonce, timeouts: timeouts ?? short).read(.first, enter: enter)
        return (record, Date().timeIntervalSince(started))
    }

    func testAnsweredReadRecordsTheGuestObservationAndAnEarlierGuestRestart() throws {
        try append("BVAGENT READY host=BRIDGEVM t=1\nstop: PSCI 0x84000009 exiting for process recreation (exit 42)\n" + synced())
        var sent: [String] = []
        let answered = read { sent.append($0); try self.reply($0); try self.result(self.body()) }
        XCTAssertEqual(sent, [command])
        XCTAssertEqual(answered.record, T17TPMBootRecord(
            boot: .first, reason: nil, tpmPresent: true, tpmReady: true, pcr0SHA256: T17TPMGuestResultTests.pcr0,
            pcr7SHA256: T17TPMGuestResultTests.pcr7, guestRestartBeforeRead: true))
    }

    func testEveryFailureIsAFixedReasonWithinItsBound() throws {
        let link = root.appendingPathComponent("elsewhere.txt")
        try Data(body().utf8).write(to: link)
        let cases: [(String, String, (String) throws -> Void, T17TPMReason)] = [
            ("asset missing", synced(Array(T17TPMBootReader.assets.dropLast())), { _ in XCTFail("launched without assets") }, .assetMissing),
            ("entry refused", synced(), { _ in throw T17Blocker(code: "ui-element-missing", detail: "x") }, .launchFailed),
            ("never answered", synced(), { _ in }, .launchFailed),
            ("launcher failed", synced(), { try self.reply($0, exit: 1) }, .launchFailed),
            ("no result", synced(), { try self.reply($0) }, .resultMissing),
            ("other nonce", synced(), { try self.reply($0); try self.result(self.body(nonce: String(repeating: "d", count: 64))) }, .resultMalformed),
            ("symlinked result", synced(), {
                try self.reply($0)
                try FileManager.default.createSymbolicLink(
                    at: self.share.appendingPathComponent("t17-tpm-boot1-5c5c5c5c5c5c.txt"), withDestinationURL: link)
            }, .resultUnsafe),
        ]
        for (name, log, enter, reason) in cases {
            try? FileManager.default.removeItem(at: runLog)
            try? FileManager.default.removeItem(at: share.appendingPathComponent("t17-tpm-boot1-5c5c5c5c5c5c.txt"))
            try append(log)
            let outcome = read(enter)
            XCTAssertEqual(outcome.record, T17TPMBootRecord(boot: .first, reason: reason, guestRestartBeforeRead: false), name)
            XCTAssertLessThan(outcome.seconds, 4, name)
        }
        try FileManager.default.removeItem(at: runLog)
        XCTAssertEqual(read { _ in XCTFail("launched without a run log") }.record, .unavailable(.first, .assetMissing))
    }

    func testAPartialResultIsReadAgainUntilItIsWhole() throws {
        try append(synced())
        let whole = body(), target = share.appendingPathComponent("t17-tpm-boot1-5c5c5c5c5c5c.txt")
        let outcome = read(timeouts: T17TPMTimeouts(asset: 5, launch: 5, result: 5)) {
            try self.reply($0)
            try self.result(String(whole.prefix(40)))
            DispatchQueue.global().asyncAfter(deadline: .now() + 0.15) { try? Data(whole.utf8).write(to: target, options: .atomic) }
        }
        XCTAssertNil(outcome.record.reason)
        XCTAssertEqual(outcome.record.pcr0SHA256, T17TPMGuestResultTests.pcr0)
    }

    func testGuestAssetsAreStagedBestEffort() throws {
        let app = root.appendingPathComponent("BridgeVM.app"), assets = app.appendingPathComponent("Contents/Resources/scripts/win-assets")
        try FileManager.default.createDirectory(at: assets, withIntermediateDirectories: true)
        XCTAssertEqual(T17TPMGuestAssets.names, ["bv-product-e2e-tpm.ps1", "bv-product-e2e-tpm.cs"])
        T17TPMGuestAssets.stage(appBundlePath: app.path, sharePath: share.path)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: share.path), [])
        try Data("script\r\n".utf8).write(to: assets.appendingPathComponent("bv-product-e2e-tpm.ps1"))
        try Data(repeating: 0x20, count: T17TPMGuestAssets.maximumBytes + 1).write(to: assets.appendingPathComponent("bv-product-e2e-tpm.cs"))
        T17TPMGuestAssets.stage(appBundlePath: app.path, sharePath: share.path)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: share.path), ["bv-product-e2e-tpm.ps1"])
        XCTAssertEqual(try Data(contentsOf: share.appendingPathComponent("bv-product-e2e-tpm.ps1")), Data("script\r\n".utf8))
        try FileManager.default.removeItem(at: assets.appendingPathComponent("bv-product-e2e-tpm.cs"))
        try FileManager.default.createSymbolicLink(at: assets.appendingPathComponent("bv-product-e2e-tpm.cs"),
                                                   withDestinationURL: assets.appendingPathComponent("bv-product-e2e-tpm.ps1"))
        T17TPMGuestAssets.stage(appBundlePath: app.path, sharePath: share.path)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: share.path), ["bv-product-e2e-tpm.ps1"])
    }

    func testFirstBootIsReadAfterTheInputProofAndTheJourneyGoesOn() throws {
        let (ui, log, error) = try journeyToClipboard(answerTPM: true)
        XCTAssertEqual(error, T17Blocker(code: "guest-evidence-missing", detail: "host clipboard challenge bytes changed"))
        XCTAssertEqual(Array(ui.events.prefix(1)), ["launch KeyboardPointer"])
        XCTAssertEqual(ui.events.last, "launch TpmBoot1")
        XCTAssertNil(log.evidence.boots[0].reason)
        XCTAssertEqual(log.evidence.boots[0].guestRestartBeforeRead, false)
        XCTAssertEqual(log.evidence.boots.dropFirst().map(\.reason), [.notReached, .notReached])
    }

    func testAnUnansweredTPMReadDoesNotStopTheJourney() throws {
        let (ui, log, error) = try journeyToClipboard(answerTPM: false)
        XCTAssertEqual(error, T17Blocker(code: "guest-evidence-missing", detail: "host clipboard challenge bytes changed"))
        XCTAssertEqual(ui.events.last, "launch TpmBoot1")
        XCTAssertEqual(log.evidence.boots[0], T17TPMBootRecord(boot: .first, reason: .launchFailed, guestRestartBeforeRead: false))
    }

    /// Runs the shared journey from first READY until its clipboard step, which this
    /// fixture fails on purpose, after the keyboard and pointer proof.
    private func journeyToClipboard(answerTPM: Bool) throws -> (JourneyUI, T17TPMEvidenceLog, T17Blocker?) {
        try append("BVAGENT READY T17 v3-share2\n" + synced(["bv-product-e2e-launch.ps1", "bv-product-e2e.ps1"]
            + T17TPMBootReader.assets.dropFirst()))
        try Data("not the challenge\n".utf8).write(to: share.appendingPathComponent("t17-clipboard-host-5c5c5c5c5c5c.txt"))
        let ui = JourneyUI(runLog: runLog, share: share, nonce: nonce, answerTPM: answerTPM)
        let log = T17TPMEvidenceLog(timeouts: short)
        let journey = T17GuestJourney(request: JourneyRequest(root: root, nonce: nonce), ui: ui, fileManager: .default, tpm: log)
        var proven: [T17GuestStage] = []
        var failure: T17Blocker?
        XCTAssertThrowsError(try journey.run(firstReady: "BVAGENT READY T17 v3-share2") { proven.append($0) }) {
            failure = $0 as? T17Blocker
        }
        XCTAssertEqual(proven, [.keyboardPointer])
        return (ui, log, failure)
    }
}

private struct JourneyRequest: T17JourneyRequest {
    let root: URL; let nonce: String
    var jobID: String { "t17-tpm" }; var commit: String { String(repeating: "0", count: 40) }
    var lane: Int { 1 }; var vmSlug: String { "t17-tpm" }
    var libraryRootPath: String { root.appendingPathComponent("library").path }
    var sharePath: String { root.appendingPathComponent("share").path }
    var diskPath: String { root.appendingPathComponent("bundle/disks/hvf-target.raw").path }
    var varsPath: String { root.appendingPathComponent("bundle/metadata/hvf-vars.fd").path }
    var snapshotPath: String { root.appendingPathComponent("bundle/metadata/snapshots/latest.snapshot").path }
    var guestEvidencePath: String { root.appendingPathComponent("bundle/metadata/product-e2e-guest-evidence.json").path }
    var bundlePath: String { root.appendingPathComponent("bundle").path }
}

/// Models the guest: launches reply at once, the input form shows itself, and the TPM
/// workload either writes its result or never answers.
private final class JourneyUI: T17UIControlling {
    var events: [String] = []
    let runLog: URL, share: URL, nonce: String, answerTPM: Bool
    private var command = ""
    init(runLog: URL, share: URL, nonce: String, answerTPM: Bool) {
        self.runLog = runLog; self.share = share; self.nonce = nonce; self.answerTPM = answerTPM
    }
    private var prefix: String { String(nonce.prefix(12)) }
    func fill(_ value: String, identifier: String, timeout: TimeInterval) throws {
        if identifier == T17RuntimeControlInput.input { command = value }
    }
    func press(_ identifier: String, timeout: TimeInterval) throws {
        if identifier == "bridgevm.runtime.keyboard.send" {
            try Data("bridgevm-t17-keyboard-pointer-v1\n\(nonce)\n".utf8).write(to: share.appendingPathComponent("t17-keyboard-pointer-\(prefix).txt"))
        }
        guard identifier == T17RuntimeControlInput.send else { return }
        let action = command.components(separatedBy: " -Action ")[1].components(separatedBy: " ")[0]
        events.append("launch \(action)")
        if action == "TpmBoot1" && !answerTPM { return }
        let handle = try FileHandle(forWritingTo: runLog); defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data("BVAGENT CMD \(command) exit=0\nT17-LAUNCHED-\(action)-\(prefix) pid=42\nBVAGENT END \(command)\n".utf8))
        if action == "KeyboardPointer" {
            try Data("bridgevm-t17-keyboard-pointer-ready-v1\n\(nonce)\n".utf8).write(to: share.appendingPathComponent("t17-keyboard-pointer-ready-\(prefix).txt"))
        } else if action == "TpmBoot1" {
            try Data(T17TPMGuestResultTests.body(nonce: nonce).utf8).write(to: share.appendingPathComponent("t17-tpm-boot1-\(prefix).txt"))
        }
    }
    func clickDisplaySurface(at spot: CGPoint, timeout: TimeInterval) throws { try T17PointerReceiptFixture.click(runLog) }
    func waitFor(_ identifier: String, timeout: TimeInterval) throws {}
    func expand(_ identifier: String, timeout: TimeInterval) throws {}
    func setText(_ value: String, identifier: String, timeout: TimeInterval) throws {}
    func setToggle(_ enabled: Bool, identifier: String, timeout: TimeInterval) throws {}
    func choose(path: String, from identifier: String, timeout: TimeInterval) throws {}
    func text(_ identifier: String, timeout: TimeInterval) throws -> String { "" }
    func optionalTexts(_ identifiers: Set<String>) throws -> [String: String] { [:] }
    func textSnapshot() -> [String] { [] }
}
