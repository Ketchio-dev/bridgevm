import Foundation
import XCTest
@testable import BridgeVMControl

/// An older resident agent fails a `scrollunits:` request, and a failed request stops ordered input, so the
/// host sends raw units only to an agent that advertised SCROLL_UNITS and whole `scroll:` notches otherwise.
final class HvfScrollUnitsRoutingTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 9_000)
    private let binding = ["disk", "vars", "evidence", "control"]

    func testOnlyAnAgentWithTheMarkerTakesScrollUnits() {
        for marker in [false, true] {
            var router = HvfSessionInputRouter()
            router.beginOwnedBoot(binding: binding)
            var command = ""
            _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { command = $0; return true }
            XCTAssertFalse(router.scrollUnits, "still negotiating")
            let id = command.split(separator: " ").last.map(String.init) ?? "missing"
            let lines = ["BVAGENT CMD \(command) exit=0", "BVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536"]
                + (marker ? ["BVINPUT_POINTER \(id) SCROLL_UNITS"] : []) + ["BVAGENT END \(command)"]
            _ = router.poll(binding: binding, serviceReady: true, lines: lines, now: now) { _ in false }
            _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { _ in false }
            XCTAssertEqual(router.state, .ready)
            XCTAssertEqual(router.scrollUnits, marker)
            XCTAssertEqual(router.route(.pointer("scrollunits:-12,5@0x0"), binding: binding, now: now), marker ? .queued : .refused)
            XCTAssertEqual(router.route(.pointer("scroll:-1@0x0"), binding: binding, now: now), .queued, "notches still work")
            XCTAssertFalse(router.failed)
        }
        var attached = HvfSessionInputRouter()
        attached.attachUnknown(binding: binding)
        XCTAssertFalse(attached.scrollUnits, "the VMM's HID wheel takes notches only")
    }

    @MainActor
    func testTheSessionSendsUnitsOrWholeNotchesByAgentSupport() throws {
        let size = CGSize(width: 800, height: 600), image = CGSize(width: 1600, height: 1200), at = CGPoint(x: 400, y: 300)
        // 5 points up and 10 points toward the left at 2 guest pixels per point: 16 units up, 32 units left.
        let trackpad = HvfScrollAccumulator.Delta(x: 10, y: 5, precise: true)
        try withNegotiatedSession(scrollUnits: true) { session, sent in
            XCTAssertTrue(session.sendPointerScroll(trackpad, location: at, viewSize: size, imageSize: image))
            XCTAssertEqual(try sent(), "scrollunits:16,-32@16384x16384")
        }
        try withNegotiatedSession(scrollUnits: false) { session, sent in
            XCTAssertFalse(session.sendPointerScroll(trackpad, location: at, viewSize: size, imageSize: image),
                           "16 units are no whole notch")
            let wheel = HvfScrollAccumulator.Delta(x: 0, y: 1, precise: false)
            XCTAssertTrue(session.sendPointerScroll(wheel, location: at, viewSize: size, imageSize: image))
            XCTAssertEqual(try sent(), "scroll:1@16384x16384")
        }
    }

    @MainActor
    private func withNegotiatedSession(scrollUnits: Bool,
                                       _ body: (HvfEngineSession, () throws -> String?) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let control = root.appendingPathComponent("agent.ctl")
        let config = HvfEngineConfig(targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true, shareHostDir: nil, shareGuestDir: nil,
            virtioNet: true, virtioGpu3d: false, nvmeBufferedIO: true, ctlFilePath: control.path)
        let log = root.appendingPathComponent("run.log")
        try "BVAGENT READY host=test t=1\nBVAGENT SERVICE start t=2\n".write(to: log, atomically: true, encoding: .utf8)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in true }
        XCTAssertTrue(session.attachToRunningVM())
        // Synthetic owned-boot boundary, not a real VM or Windows receipt.
        session.beginOwnedInputBoot()
        session.poll()
        let command = try String(contentsOf: control, encoding: .utf8).split(separator: "\n").last.map(String.init) ?? "missing"
        let id = command.split(separator: " ").last ?? "missing"
        let marker = scrollUnits ? "BVINPUT_POINTER \(id) SCROLL_UNITS\n" : ""
        let handle = try FileHandle(forWritingTo: log)
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(("BVAGENT CMD \(command) exit=0\nBVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536\n"
            + marker + "BVAGENT END \(command)\n").utf8))
        try handle.close()
        session.poll(); session.poll()
        XCTAssertEqual(session.inputDriver.router.scrollUnits, scrollUnits)
        // The pointer request the agent was sent last, decoded.
        try body(session) {
            let last = try String(contentsOf: control, encoding: .utf8).split(separator: "\n").last.map(String.init) ?? ""
            let fields = last.split(separator: " ")
            guard fields.count == 3, fields[0] == "POINTERINPUT", let data = Data(base64Encoded: String(fields[2])) else { return nil }
            return String(data: data, encoding: .utf8)
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("input.ctl").path),
                       "nothing went to the VMM's legacy input")
    }
}
