import Foundation
import XCTest
@testable import BridgeVMControl

/// A guest reset record ends an activated ordered-input generation. The next generation of the
/// same owned binding behaves like a fresh owned boot: legacy until its own INPUTCAPS handshake,
/// which waits for its own `BVAGENT SERVICE start`, and never a replay of cancelled requests.
@MainActor
final class HvfGuestResetInputGenerationTests: XCTestCase {
    private typealias Gate = HvfEngineSessionGuestResetGateTests
    private let now = Date(timeIntervalSince1970: 6_000)
    private let binding = ["disk", "vars", "evidence", "control"]
    private let records = [HvfGuestResetRecord.processRecreation, "PSCI SYSTEM_RESET: reboot 1/8",
                           "stop: PSCI 0x84000009 max reboot count 8 reached"]
    private let active = BvAgentEvent.unknown("ordered input active: receipts confirm insertion, not application consumption")

    func testNextGenerationRenegotiatesAfterItsServiceStartAndNeverReplays() throws {
        for reset in Gate.resets {
            let fixture = try HvfRuntimeKeyboardDraftFixture(); defer { fixture.remove() }
            try fixture.withSession { session in
                try fixture.negotiateOrderedInput(session)
                XCTAssertEqual(session.sendText("old"), .acceptedForProcessing, reset)
                XCTAssertEqual(try fixture.commands().count, 2, reset)
                try append(reset, fixture); session.poll()
                try append("BVAGENT READY host=t t=1\n", fixture); session.poll(); session.poll()
                XCTAssertEqual(try fixture.commands().count, 2, "nothing ordered before SERVICE start: \(reset)")
                try append("BVAGENT SERVICE start t=2\n", fixture); session.poll(); session.poll()
                let commands = try fixture.commands()
                XCTAssertEqual(commands.count, 3, reset)
                let query = commands.last ?? ""
                XCTAssertTrue(query.hasPrefix("INPUTCAPS "), "next generation negotiates: \(reset)")
                if query.hasPrefix("INPUTCAPS ") {
                    let id = query.split(separator: " ")[1]
                    try append("BVAGENT CMD \(query) exit=0\nBVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536\n"
                        + "BVAGENT END \(query)\n", fixture)
                    session.poll(); session.poll()
                }
                XCTAssertEqual(session.events.filter { $0 == active }.count, 2, reset)
                var draft = "new"
                XCTAssertEqual(HvfKeyboardDraft.submit(&draft, to: session), .acceptedForProcessing, reset)
                let texts = try fixture.commands().filter { $0.hasPrefix("TEXTINPUT ") }
                XCTAssertEqual(texts.count, 2, "the cancelled request is never replayed: \(reset)")
                XCTAssertEqual(texts.last?.split(separator: " ").last, "bmV3", reset)
                XCTAssertNil(try fixture.bytes(fixture.input), reset)
            }
        }
    }

    /// Like a fresh owned boot, HID input before the handshake only delays the switch (HvfLegacyInputActivity).
    func testNextGenerationUsesLegacyInputUntilItsHandshakeLikeAFreshOwnedBoot() throws {
        for reset in Gate.resets {
            let fixture = try HvfRuntimeKeyboardDraftFixture(); defer { fixture.remove() }
            try fixture.withSession { session in
                try fixture.negotiateOrderedInput(session)
                try append(reset, fixture); session.poll()
                session.sendKey("a") // e.g. the next boot's sign-in screen
                XCTAssertEqual(try fixture.bytes(fixture.input), Data("KEY a\n".utf8), reset)
                try append(Gate.boot, fixture); session.poll(); session.poll()
                let commands = try fixture.commands()
                XCTAssertEqual(commands.count, 2, "the next generation negotiates despite that key: \(reset)")
                XCTAssertTrue(commands.last?.hasPrefix("INPUTCAPS ") == true, reset)
                session.sendKey("b")
                XCTAssertEqual(try fixture.bytes(fixture.input), Data("KEY a\nKEY b\n".utf8), "HID until the handshake: \(reset)")
            }
        }
    }

    func testChangedBindingStaysRefusedAcrossTheNextGeneration() throws {
        for reset in Gate.resets {
            let fixture = try HvfRuntimeKeyboardDraftFixture(); defer { fixture.remove() }
            try fixture.withSession { session in
                try fixture.negotiateOrderedInput(session)
                session.config.uefiVarsPath = fixture.root.appendingPathComponent("other-vars.fd").path
                try append(reset, fixture); session.poll()
                try append(Gate.boot, fixture); session.poll(); session.poll()
                XCTAssertEqual(try fixture.commands().count, 1, reset)
                XCTAssertEqual(session.sendText("x"), .refused, reset)
                session.sendKey("a")
                XCTAssertNil(try fixture.bytes(fixture.input), reset)
            }
        }
    }

    func testResetRecordInTheBatchReArmsOnlyForTheSameBinding() {
        for reset in records {
            var router = readyRouter()
            XCTAssertEqual(router.route(.text("pending"), binding: binding, now: now), .queued)
            XCTAssertEqual(router.pollBatch(binding: binding, serviceReady: false, lines: ["BVAGENT re-READY t t=3", reset],
                                            now: now) { _ in XCTFail("sent across \(reset)"); return true },
                           .cancelled(.sessionChanged, discarded: 1), reset)
            XCTAssertEqual([router.failed, router.activated, router.eligible], [false, false, true], reset)
            XCTAssertEqual(router.count, 0, reset); XCTAssertEqual(router.state, .disconnected, reset)
            for lines in [[], ["BVAGENT READY host=t t=1"]] {
                _ = router.pollBatch(binding: binding, serviceReady: false, lines: lines, now: now) {
                    _ in XCTFail("sent before SERVICE start: \(reset)"); return true
                }
            }
            _ = router.pollBatch(binding: binding, serviceReady: true, lines: ["BVAGENT SERVICE start t=2"], now: now) {
                _ in XCTFail("sent in the SERVICE start batch: \(reset)"); return true
            }
            var query = ""
            _ = router.pollBatch(binding: binding, serviceReady: true, lines: [], now: now) { query = $0; return true }
            XCTAssertTrue(query.hasPrefix("INPUTCAPS "), reset)
            XCTAssertEqual(router.route(.key("a"), binding: binding, now: now), .legacy, "fresh boot before handshake")
            var other = readyRouter()
            _ = other.poll(binding: binding, serviceReady: false, lines: [reset], now: now) { _ in false }
            XCTAssertEqual(other.failure, .guestReset, reset)
            other.beginNextGeneration(binding: ["other"], lines: [reset])
            XCTAssertTrue(other.failed, reset)
            XCTAssertEqual(other.route(.key("a"), binding: ["other"], now: now), .refused, reset)
            var rearmed = readyRouter()
            _ = rearmed.pollBatch(binding: binding, serviceReady: false, lines: [reset], now: now) { _ in false }
            XCTAssertEqual(rearmed.route(.key("a"), binding: ["other"], now: now), .refused, reset)
            XCTAssertEqual(rearmed.failure, .bindingChanged, reset)
        }
    }

    func testOnlyAWholeResetRecordReArmsAndNeverAChangedBinding() {
        // A lost service is the host's: an agent line does not end it either (an agent restart ends only the old
        // agent instance's failures, HvfLateGuestResetInputTests).
        let inert = ["PSCI SYSTEM_RESET: requested", "guest " + HvfGuestResetRecord.processRecreation, "BVAGENT READY host=t t=1"]
        var router = readyRouter()
        _ = router.pollBatch(binding: binding, serviceReady: false, lines: [], now: now) { _ in false }
        _ = router.pollBatch(binding: binding, serviceReady: true, lines: inert, now: now) {
            _ in XCTFail("sent without a reset record"); return true
        }
        XCTAssertTrue(router.failed)
        XCTAssertEqual(router.route(.key("enter"), binding: binding, now: now), .refused)
        let failures: [HvfSessionInputFailure] = [.guestReset, .agentRestarted, .serviceLost, .requestFailed, .bindingChanged,
                                                  .unavailable]
        XCTAssertEqual(failures.map(\.endsAtGuestReset), [true, true, true, true, false, false])
        XCTAssertEqual(failures.map(\.endsAtAgentRestart), [false, true, false, true, false, true])
        var changed = readyRouter()
        _ = changed.pollBatch(binding: ["other"], serviceReady: false, lines: [records[0]], now: now) { _ in false }
        XCTAssertEqual(changed.failure, .bindingChanged)
        changed.beginNextGeneration(binding: binding, lines: [records[0]])
        XCTAssertTrue(changed.failed)
        XCTAssertEqual(changed.route(.key("enter"), binding: binding, now: now), .refused)
        XCTAssertFalse(changed.allowLegacyWrite("KEY enter", binding: binding, now: now))
    }

    func testUnownedBootsAreNotReArmedAndHIDInputOnlyDelaysAnOwnedBootsSwitch() {
        let reset = records[0]
        var attached = HvfSessionInputRouter()
        attached.attachUnknown(binding: binding)
        _ = attached.pollBatch(binding: binding, serviceReady: false, lines: [reset], now: now) { _ in false }
        _ = attached.pollBatch(binding: binding, serviceReady: true, lines: [], now: now) {
            _ in XCTFail("unknown history negotiated"); return true
        }
        XCTAssertEqual(attached.route(.key("a"), binding: binding, now: now), .legacy)
        var legacy = HvfSessionInputRouter()
        legacy.beginOwnedBoot(binding: binding)
        XCTAssertEqual(legacy.route(.key("a"), binding: binding, now: now), .legacy)
        XCTAssertTrue(legacy.allowLegacyWrite("KEY a", binding: binding, now: now))
        _ = legacy.pollBatch(binding: binding, serviceReady: false, lines: [reset], now: now) { _ in false }
        var query = ""
        _ = legacy.pollBatch(binding: binding, serviceReady: true, lines: [], now: now) { query = $0; return true }
        XCTAssertTrue(query.hasPrefix("INPUTCAPS "), "an owned boot negotiates despite earlier HID input")
        let id = query.split(separator: " ").last ?? "missing"
        _ = legacy.pollBatch(binding: binding, serviceReady: true, lines: [
            "BVAGENT CMD \(query) exit=0", "BVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536", "BVAGENT END \(query)",
        ], now: now) { _ in false }
        XCTAssertTrue(legacy.eligible)
        XCTAssertEqual(legacy.route(.key("b"), binding: binding, now: now), .legacy, "input.ctl may still be unconsumed")
        _ = legacy.pollBatch(binding: binding, serviceReady: true, lines: [], now: now.addingTimeInterval(1)) { _ in false }
        XCTAssertEqual(legacy.route(.key("enter"), binding: binding, now: now), .queued, "a quiet second later")
    }

    private func readyRouter() -> HvfSessionInputRouter {
        var router = HvfSessionInputRouter()
        router.beginOwnedBoot(binding: binding)
        var query = ""
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { query = $0; return true }
        let id = query.split(separator: " ").last ?? "missing"
        _ = router.poll(binding: binding, serviceReady: true, lines: [
            "BVAGENT CMD \(query) exit=0", "BVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536", "BVAGENT END \(query)",
        ], now: now) { _ in false }
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { _ in false }
        XCTAssertTrue(router.activated)
        return router
    }

    private func append(_ text: String, _ fixture: HvfRuntimeKeyboardDraftFixture) throws {
        let log = fixture.evidence.appendingPathComponent("run.log")
        let handle = try FileHandle(forWritingTo: log)
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(text.utf8))
    }
}
