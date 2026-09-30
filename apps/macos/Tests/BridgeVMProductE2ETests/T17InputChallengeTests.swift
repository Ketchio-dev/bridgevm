import XCTest
@testable import BridgeVMProductE2E

final class T17InputChallengeTests: XCTestCase {
    private let nonce = String(repeating: "5c", count: 32)
    private var root: URL!
    private var share: URL { root.appendingPathComponent("share") }
    private var ready: URL { share.appendingPathComponent("t17-keyboard-pointer-ready-5c5c5c5c5c5c.txt") }
    private var readyBody: Data { Data("bridgevm-t17-keyboard-pointer-ready-v1\n\(nonce)\n".utf8) }

    override func setUpWithError() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent("t17-input-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: share, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: root.appendingPathComponent("bundle/logs/hvf"), withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws { try? FileManager.default.removeItem(at: root) }

    func testJourneySendsHostInputOnlyAfterTheGuestFormIsShown() throws {
        let log = root.appendingPathComponent("bundle/logs/hvf/run.log")
        try Data(("BVAGENT READY T17 v3-share2\nBVAGENT SHARE host->guest bv-product-e2e-launch.ps1 bytes=1 t=1\n"
            + "BVAGENT SHARE host->guest bv-product-e2e.ps1 bytes=1 t=2\n").utf8).write(to: log)
        let ui = GuestUI(runLog: log, ready: ready, readyBody: readyBody, readyDelay: 0.6,
                         output: share.appendingPathComponent("t17-keyboard-pointer-5c5c5c5c5c5c.txt"))
        let journey = T17GuestJourney(request: Request(root: root, nonce: nonce), ui: ui, fileManager: .default)
        XCTAssertThrowsError(try journey.run(firstReady: "BVAGENT READY T17 v3-share2") { throw Stop.after($0) }) {
            XCTAssertEqual($0 as? Stop, .after(.keyboardPointer))
        }
        XCTAssertEqual(ui.events, [
            "launch KeyboardPointer", "press bridgevm.runtime.display.open shown=true", "click shown=true",
            "fill bridgevm.runtime.keyboard.input t17kbd5c5c5c5c5c5c", "press bridgevm.runtime.keyboard.send shown=true",
        ])
    }

    func testMissingMarkerFailsBeforeAnyInput() {
        let ui = challengeUI()
        XCTAssertThrowsError(try challenge(ui).deliver()) {
            XCTAssertEqual($0 as? T17Blocker, T17Blocker(code: "guest-evidence-missing", detail: "guest input challenge was not shown"))
        }
        XCTAssertEqual(ui.events, [])
    }

    func testMarkerForAnotherNonceFailsBeforeAnyInput() throws {
        try Data("bridgevm-t17-keyboard-pointer-ready-v1\n\(String(repeating: "5c", count: 31))00\n".utf8).write(to: ready)
        let ui = challengeUI()
        XCTAssertThrowsError(try challenge(ui).deliver()) {
            XCTAssertEqual(($0 as? T17Blocker)?.detail, "guest input challenge ready marker is unsafe or changed")
        }
        XCTAssertEqual(ui.events, [])
    }

    func testSymlinkedMarkerFailsBeforeAnyInput() throws {
        let target = root.appendingPathComponent("elsewhere.txt")
        try readyBody.write(to: target)
        try FileManager.default.createSymbolicLink(at: ready, withDestinationURL: target)
        let ui = challengeUI()
        XCTAssertThrowsError(try challenge(ui).deliver()) {
            XCTAssertEqual(($0 as? T17Blocker)?.detail, "guest input challenge ready marker is unsafe or changed")
        }
        XCTAssertEqual(ui.events, [])
    }

    func testShownMarkerReleasesInputInOrder() throws {
        try readyBody.write(to: ready)
        let ui = challengeUI()
        try challenge(ui).deliver()
        XCTAssertEqual(ui.events, [
            "press bridgevm.runtime.display.open shown=true", "click shown=true",
            "fill bridgevm.runtime.keyboard.input t17kbd5c5c5c5c5c5c", "press bridgevm.runtime.keyboard.send shown=true",
        ])
    }

    private func challengeUI() -> GuestUI {
        GuestUI(runLog: root.appendingPathComponent("unused.log"), ready: ready, readyBody: readyBody, readyDelay: nil,
                output: share.appendingPathComponent("t17-keyboard-pointer-5c5c5c5c5c5c.txt"))
    }

    private func challenge(_ ui: GuestUI) -> T17InputChallenge {
        T17InputChallenge(sharePath: share.path, nonce: nonce, ui: ui, runLog: ui.runLog, readyTimeout: 0.5)
    }
}

private enum Stop: Error, Equatable { case after(T17GuestStage) }

private struct Request: T17JourneyRequest {
    let root: URL; let nonce: String
    var jobID: String { "t17-input" }; var commit: String { String(repeating: "0", count: 40) }
    var lane: Int { 1 }; var vmSlug: String { "t17-input" }
    var libraryRootPath: String { root.appendingPathComponent("library").path }
    var sharePath: String { root.appendingPathComponent("share").path }
    var diskPath: String { root.appendingPathComponent("bundle/disks/hvf-target.raw").path }
    var varsPath: String { root.appendingPathComponent("bundle/metadata/hvf-vars.fd").path }
    var snapshotPath: String { root.appendingPathComponent("bundle/metadata/snapshots/latest.snapshot").path }
    var guestEvidencePath: String { root.appendingPathComponent("bundle/metadata/product-e2e-guest-evidence.json").path }
    var bundlePath: String { root.appendingPathComponent("bundle").path }
}

/// Models the guest: the launcher replies at once, and the challenge form is shown later.
private final class GuestUI: T17UIControlling {
    var events: [String] = []
    let runLog: URL, ready: URL, readyBody: Data, readyDelay: TimeInterval?, output: URL
    private var command = ""
    init(runLog: URL, ready: URL, readyBody: Data, readyDelay: TimeInterval?, output: URL) {
        self.runLog = runLog; self.ready = ready; self.readyBody = readyBody; self.readyDelay = readyDelay; self.output = output
    }
    private var shown: Bool { (try? Data(contentsOf: ready)) == readyBody }
    func waitFor(_ identifier: String, timeout: TimeInterval) throws {}
    func fill(_ value: String, identifier: String, timeout: TimeInterval) throws {
        if identifier == T17RuntimeControlInput.input { command = value } else { events.append("fill \(identifier) \(value)") }
    }
    func press(_ identifier: String, timeout: TimeInterval) throws {
        guard identifier == T17RuntimeControlInput.send else {
            events.append("press \(identifier) shown=\(shown)")
            if identifier == "bridgevm.runtime.keyboard.send", shown {
                try Data("bridgevm-t17-keyboard-pointer-v1\n\(String(repeating: "5c", count: 32))\n".utf8).write(to: output)
            }
            return
        }
        let action = command.components(separatedBy: " -Action ")[1].components(separatedBy: " ")[0]
        events.append("launch \(action)")
        let handle = try FileHandle(forWritingTo: runLog); defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data("BVAGENT CMD \(command) exit=0\nT17-LAUNCHED-\(action)-5c5c5c5c5c5c pid=42\nBVAGENT END \(command)\n".utf8))
        if let readyDelay {
            let ready = ready, body = readyBody
            DispatchQueue.global().asyncAfter(deadline: .now() + readyDelay) { try? body.write(to: ready) }
        }
    }
    func clickDisplaySurface(timeout: TimeInterval) throws { events.append("click shown=\(shown)"); try T17PointerReceiptFixture.click(runLog) }
    func expand(_ identifier: String, timeout: TimeInterval) throws {}
    func setText(_ value: String, identifier: String, timeout: TimeInterval) throws {}
    func setToggle(_ enabled: Bool, identifier: String, timeout: TimeInterval) throws {}
    func choose(path: String, from identifier: String, timeout: TimeInterval) throws {}
    func text(_ identifier: String, timeout: TimeInterval) throws -> String { "" }
    func optionalTexts(_ identifiers: Set<String>) throws -> [String: String] { [:] }
    func textSnapshot() -> [String] { [] }
}
