import Foundation
import XCTest
@testable import BridgeVMControl

// Reset records exactly as the HVF runtime prints them into run.log: the
// exit-for-recreation stop of product runs (BRIDGEVM_EXIT_ON_RESET=1), the
// wrapper's in-process reboot line and the reboot-limit stop. No runtime path
// prints "PSCI_SYSTEM_RESET"; the "max reboots" banner starts every generation.
final class HvfGuestResetRecordTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 5_000)
    private let binding = ["disk", "vars", "evidence", "control"]
    private let recreation = "stop: PSCI 0x84000009 exiting for process recreation (exit 42)"
    private var resets: [String] {
        [recreation, "PSCI SYSTEM_RESET: reboot 1/8", "stop: PSCI 0x84000009 max reboot count 8 reached"]
            .flatMap { [$0, $0 + "\r"] }
    }
    private var inert: [String] {
        ["PSCI_SYSTEM_RESET", "PSCI SYSTEM_RESET: requested", "PSCI SYSTEM_RESET max reboots: 8",
         "PSCI SYSTEM_RESET: reboot 1/8 extra", "guest " + recreation, recreation + " extra",
         "stop: PSCI 0x84000008 (system off)"]
    }

    func testRequestsFailAsRestartedWhenARuntimeResetRecordFollowsTheirReceipt() throws {
        for reset in resets {
            var text = try XCTUnwrap(HvfUnicodeInputRequest(text: "a", now: now))
            XCTAssertEqual(text.consume(lines: receipt(text.command) + [reset], now: now), .failed(.restarted), reset)
            var caps = HvfInputCapabilitiesRequest(now: now)
            XCTAssertEqual(caps.consume(lines: support(caps.command) + [reset], now: now), .failed(.restarted), reset)
            var inventory = HvfWindowInventoryRequest(now: now)
            if case .failure(.restarted) = inventory.consume(lines: windows(inventory.command) + [reset], now: now) {}
            else { XCTFail("inventory published across \(reset)") }
            var paste = HvfClipboardPaste(base64: "YWJj", now: now)
            XCTAssertEqual(paste.consume(lines: pasted(paste) + [reset], now: now), false, reset)
        }
    }

    func testStreamsCancelAndNeverSendAcrossARuntimeResetRecord() {
        for reset in resets {
            var stream = HvfAcknowledgedInputStream()
            XCTAssertTrue(stream.enqueue(.text("a"), now: now))
            var command = ""
            _ = stream.advance(lines: [], now: now) { command = $0; return true }
            XCTAssertEqual(stream.advance(lines: receipt(command) + [reset], now: now) { _ in false },
                           .cancelled(.sessionChanged, discarded: 1), reset)
            var negotiated = readyStream()
            XCTAssertEqual(negotiated.enqueue(.key("enter"), now: now), .queued)
            XCTAssertEqual(negotiated.poll(serviceReady: true, legacyQuiescent: true, lines: [reset], now: now) {
                _ in XCTFail("sent across \(reset)"); return true
            }, .cancelled(.sessionChanged, discarded: 1), reset)
            XCTAssertEqual(negotiated.state, .disconnected, reset)
            var router = readyRouter()
            XCTAssertEqual(router.route(.text("pending"), binding: binding, now: now), .queued)
            _ = router.poll(binding: binding, serviceReady: true, lines: [reset], now: now) {
                _ in XCTFail("sent across \(reset)"); return true
            }
            XCTAssertTrue(router.failed, reset)
            XCTAssertEqual(router.count, 0, reset)
            XCTAssertEqual(router.route(.key("enter"), binding: binding, now: now), .refused, reset)
        }
    }

    func testRetiredAndInexactResetTextLeavesCompletedWorkIntact() throws {
        for line in inert {
            var text = try XCTUnwrap(HvfUnicodeInputRequest(text: "a", now: now))
            XCTAssertEqual(text.consume(lines: receipt(text.command) + [line], now: now), .inserted, line)
            var caps = HvfInputCapabilitiesRequest(now: now)
            XCTAssertEqual(caps.consume(lines: support(caps.command) + [line], now: now), .supported, line)
            var inventory = HvfWindowInventoryRequest(now: now)
            if case .success = inventory.consume(lines: windows(inventory.command) + [line], now: now) {}
            else { XCTFail("inventory discarded on \(line)") }
            var paste = HvfClipboardPaste(base64: "YWJj", now: now)
            XCTAssertEqual(paste.consume(lines: pasted(paste) + [line], now: now), true, line)
            var stream = HvfAcknowledgedInputStream()
            XCTAssertTrue(stream.enqueue(.text("a"), now: now))
            var command = ""
            guard case let .sent(id) = stream.advance(lines: [], now: now, send: { command = $0; return true }) else {
                XCTFail("request not sent"); continue
            }
            XCTAssertEqual(stream.advance(lines: receipt(command) + [line], now: now) { _ in false }, .inserted(id), line)
            var router = readyRouter()
            _ = router.poll(binding: binding, serviceReady: true, lines: [line], now: now) { _ in false }
            XCTAssertFalse(router.failed, line)
            XCTAssertEqual(router.route(.text("next"), binding: binding, now: now), .queued, line)
        }
    }

    @MainActor
    func testInventoryPanelClearsWhenARuntimeResetRecordEndsTheBatch() throws {
        for reset in [recreation, "PSCI SYSTEM_RESET: reboot 1/8"] {
            let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            defer { try? FileManager.default.removeItem(at: root) }
            let log = root.appendingPathComponent("run.log")
            try "BVAGENT READY host=test t=1\nBVAGENT SERVICE start t=2\n".write(to: log, atomically: true, encoding: .utf8)
            let session = HvfEngineSession(config: HvfEngineConfig(
                targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
                watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
                shareHostDir: nil, shareGuestDir: nil, virtioNet: true, virtioGpu3d: false,
                nvmeBufferedIO: true, ctlFilePath: root.appendingPathComponent("agent.ctl").path
            ), repoRoot: root) { _ in true }
            XCTAssertTrue(session.attachToRunningVM())
            let model = HvfWindowInventoryController(session: session)
            XCTAssertTrue(model.refresh())
            let command = try String(contentsOf: root.appendingPathComponent("agent.ctl"), encoding: .utf8)
                .trimmingCharacters(in: .newlines)
            let handle = try FileHandle(forWritingTo: log)
            try handle.seekToEnd()
            try handle.write(contentsOf: Data((windows(command) + [reset]).map { $0 + "\n" }.joined().utf8))
            try handle.close()
            model.poll()
            XCTAssertTrue(model.records.isEmpty, reset)
            XCTAssertFalse(model.isLoading, reset)
            XCTAssertEqual(model.status, "Guest restarted", reset)
            model.stop()
        }
    }

    private func receipt(_ command: String) -> [String] {
        let id = command.split(separator: " ")[1]
        return ["BVAGENT CMD \(command) exit=0", "BVINPUT_INSERTED \(id) 2", "BVAGENT END \(command)"]
    }
    private func support(_ command: String) -> [String] {
        let id = command.split(separator: " ")[1]
        return ["BVAGENT CMD \(command) exit=0", "BVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536",
                "BVAGENT END \(command)"]
    }
    private func windows(_ command: String) -> [String] {
        ["BVAGENT \(command) WIN 42 7 0 0 640 480 QQ==", "BVAGENT \(command) WINEND"]
    }
    private func pasted(_ paste: HvfClipboardPaste) -> [String] {
        ["BVAGENT CMD \(paste.command) exit=0", paste.marker, "BVAGENT END \(paste.command)"]
    }
    private func readyStream() -> HvfNegotiatedInputStream {
        var stream = HvfNegotiatedInputStream()
        var query = ""
        _ = stream.poll(serviceReady: true, legacyQuiescent: true, lines: [], now: now) { query = $0; return true }
        _ = stream.poll(serviceReady: true, legacyQuiescent: true, lines: support(query), now: now) { _ in false }
        _ = stream.poll(serviceReady: true, legacyQuiescent: true, lines: [], now: now) { _ in false }
        XCTAssertEqual(stream.state, .ready)
        return stream
    }
    private func readyRouter() -> HvfSessionInputRouter {
        var router = HvfSessionInputRouter()
        router.beginOwnedBoot(binding: binding)
        var query = ""
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { query = $0; return true }
        _ = router.poll(binding: binding, serviceReady: true, lines: support(query), now: now) { _ in false }
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { _ in false }
        XCTAssertEqual(router.state, .ready)
        return router
    }
}
