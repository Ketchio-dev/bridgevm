import Foundation
import XCTest
@testable import BridgeVMControl

/// A slow Windows restart can fail an activated router before its reset record arrives: the agent is
/// gone, so queued input expires, or the agent restarts or the service is lost first. The later reset
/// record still replaces the harness and its agent.ctl queue, so the same owned binding then starts
/// the next generation as a fresh owned boot. A changed binding or a missing capability stays failed.
@MainActor
final class HvfLateGuestResetInputTests: XCTestCase {
    private typealias Gate = HvfEngineSessionGuestResetGateTests
    private let now = Date(timeIntervalSince1970: 7_000)
    private let binding = ["disk", "vars", "evidence", "control"]
    private let records = [HvfGuestResetRecord.processRecreation, "PSCI SYSTEM_RESET: reboot 1/8",
                           "stop: PSCI 0x84000009 max reboot count 8 reached"]

    func testSlowShutdownExpiryIsEndedByTheLaterResetRecord() {
        for reset in records {
            for next in [[reset], [reset, "BVAGENT READY host=t t=1", "BVAGENT SERVICE start t=2"]] {
                var router = readyRouter()
                for point in ["move:1x1", "move:2x2"] {
                    XCTAssertEqual(router.route(.pointer(point), binding: binding, now: now), .queued)
                }
                _ = router.pollBatch(binding: binding, serviceReady: true, lines: [], now: now) { _ in true }
                XCTAssertEqual(router.pollBatch(binding: binding, serviceReady: true, lines: [], now: now + 31) {
                    _ in XCTFail("sent after expiry"); return true
                }, .cancelled(.expired, discarded: 2), reset)
                XCTAssertEqual(router.failure, .requestFailed, reset)
                _ = router.pollBatch(binding: binding, serviceReady: true, lines: ["Windows is restarting"], now: now + 40) {
                    _ in XCTFail("sent while failed"); return true
                }
                XCTAssertEqual(router.route(.key("a"), binding: binding, now: now + 40), .refused, "no reset yet: \(reset)")
                _ = router.pollBatch(binding: binding, serviceReady: next.count > 1, lines: next, now: now + 60) {
                    _ in XCTFail("sent in the reset batch: \(next)"); return true
                }
                assertFreshGeneration(&router, servicePending: next.count == 1, "\(next)")
            }
        }
    }

    func testAgentRestartOrServiceLossIsEndedByTheLaterResetRecord() {
        let causes: [(HvfSessionInputFailure, Bool, [String])] = [
            (.agentRestarted, true, ["BVAGENT re-READY t t=40"]), (.agentRestarted, true, ["BVAGENT READY host=t t=1"]),
            (.agentRestarted, true, ["BVAGENT SERVICE start t=2"]), (.serviceLost, false, []),
        ]
        for reset in records {
            for (reason, ready, lines) in causes {
                var router = readyRouter()
                XCTAssertEqual(router.route(.text("pending"), binding: binding, now: now), .queued)
                _ = router.pollBatch(binding: binding, serviceReady: true, lines: [], now: now) { _ in true }
                XCTAssertEqual(router.pollBatch(binding: binding, serviceReady: ready, lines: lines, now: now) {
                    _ in XCTFail("sent across \(lines)"); return true
                }, .cancelled(.sessionChanged, discarded: 1), "\(lines)")
                XCTAssertEqual(router.failure, reason, "\(lines)")
                _ = router.pollBatch(binding: binding, serviceReady: ready, lines: ["BVAGENT SERVICE start t=3"], now: now) {
                    _ in XCTFail("sent without a reset record: \(lines)"); return true
                }
                XCTAssertEqual(router.route(.text("x"), binding: binding, now: now), .refused, "\(lines)")
                _ = router.pollBatch(binding: binding, serviceReady: false, lines: [reset], now: now) {
                    _ in XCTFail("sent in the reset batch: \(lines)"); return true
                }
                assertFreshGeneration(&router, servicePending: true, "\(reason) \(lines) \(reset)")
            }
        }
    }

    func testChangedBindingAndMissingCapabilityStayAsTheyWereAcrossALaterReset() {
        for reset in records {
            var changed = readyRouter()
            _ = changed.pollBatch(binding: ["other"], serviceReady: true, lines: [], now: now) { _ in false }
            XCTAssertEqual(changed.failure, .bindingChanged, reset)
            for next in [binding, ["other"]] {
                for (ready, lines) in [(false, [reset]), (true, ["BVAGENT SERVICE start t=2"]), (true, [])] {
                    _ = changed.pollBatch(binding: next, serviceReady: ready, lines: lines, now: now) {
                        _ in XCTFail("sent after a binding change: \(lines)"); return true
                    }
                }
                XCTAssertEqual(changed.failure, .bindingChanged, reset)
                XCTAssertEqual(changed.route(.key("a"), binding: next, now: now), .refused, reset)
                XCTAssertFalse(changed.allowLegacyWrite(binding: next), reset)
            }
            // A guest without the capability leaves its owned boot legacy; a reset does not renegotiate it.
            var missing = HvfSessionInputRouter()
            missing.beginOwnedBoot(binding: binding)
            var query = ""
            _ = missing.pollBatch(binding: binding, serviceReady: true, lines: [], now: now) { query = $0; return true }
            _ = missing.pollBatch(binding: binding, serviceReady: true, lines: ["BVAGENT CMD \(query) exit=1"], now: now) {
                _ in false
            }
            XCTAssertEqual([missing.eligible, missing.activated, missing.failed], [false, false, false], reset)
            for (ready, lines) in [(false, [reset]), (true, ["BVAGENT SERVICE start t=2"]), (true, [])] {
                _ = missing.pollBatch(binding: binding, serviceReady: ready, lines: lines, now: now) {
                    _ in XCTFail("renegotiated a guest without the capability"); return true
                }
            }
            XCTAssertEqual(missing.route(.key("a"), binding: binding, now: now), .legacy, reset)
        }
    }

    func testResetAndNextServiceStartInOneBatchRenegotiateOnTheNextPoll() throws {
        for reset in Gate.resets {
            let fixture = try HvfRuntimeKeyboardDraftFixture(); defer { fixture.remove() }
            try fixture.withSession { session in
                try fixture.negotiateOrderedInput(session)
                XCTAssertEqual(session.sendText("old"), .acceptedForProcessing, reset)
                try append(reset + Gate.boot, fixture); session.poll()
                XCTAssertEqual(try fixture.commands().count, 2, "INPUTCAPS never shares the SERVICE start batch: \(reset)")
                session.poll()
                try assertNextGenerationWorks(session, fixture, reset)
            }
        }
    }

    func testSessionResetRecordEndsAnEarlierAgentRestartOrRejectedRequest() throws {
        let failures: [(String) -> String] = [
            { _ in "BVAGENT re-READY t t=40\n" },
            { "BVAGENT CMD \($0) exit=1\nBVAGENT END \($0)\n" },
        ]
        for reset in Gate.resets {
            for failure in failures {
                let fixture = try HvfRuntimeKeyboardDraftFixture(); defer { fixture.remove() }
                try fixture.withSession { session in
                    try fixture.negotiateOrderedInput(session)
                    XCTAssertEqual(session.sendText("old"), .acceptedForProcessing, reset)
                    try append(failure(try fixture.commands().last ?? ""), fixture); session.poll()
                    XCTAssertEqual(session.sendText("lost"), .refused, "failed until the reset record: \(reset)")
                    session.sendKey("a")
                    try append(reset, fixture); session.poll()
                    try append(Gate.boot, fixture); session.poll(); session.poll()
                    try assertNextGenerationWorks(session, fixture, reset)
                }
            }
        }
    }

    /// The re-armed router sends nothing before the poll after SERVICE start, negotiates again and
    /// sends only input routed after the handshake.
    private func assertFreshGeneration(_ router: inout HvfSessionInputRouter, servicePending: Bool, _ message: String) {
        XCTAssertEqual([router.failed, router.activated, router.eligible], [false, false, true], message)
        XCTAssertEqual(router.count, 0, message)
        if servicePending {
            for (ready, lines) in [(false, ["BVAGENT READY host=t t=1"]), (true, ["BVAGENT SERVICE start t=2"])] {
                _ = router.pollBatch(binding: binding, serviceReady: ready, lines: lines, now: now + 70) {
                    _ in XCTFail("sent before the poll after SERVICE start: \(message)"); return true
                }
            }
        }
        var sent: [String] = []
        _ = router.pollBatch(binding: binding, serviceReady: true, lines: [], now: now + 80) { sent.append($0); return true }
        let query = sent.first ?? ""
        XCTAssertTrue(query.hasPrefix("INPUTCAPS "), message)
        let id = query.split(separator: " ").last ?? "missing"
        for lines in [["BVAGENT CMD \(query) exit=0", "BVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536",
                       "BVAGENT END \(query)"], []] {
            _ = router.pollBatch(binding: binding, serviceReady: true, lines: lines, now: now + 80) { sent.append($0); return true }
        }
        XCTAssertTrue(router.activated, message)
        XCTAssertEqual(router.route(.key("enter"), binding: binding, now: now + 80), .queued, message)
        _ = router.pollBatch(binding: binding, serviceReady: true, lines: [], now: now + 80) { sent.append($0); return true }
        XCTAssertEqual(sent.count, 2, "only INPUTCAPS and the new key; nothing resent: \(message)")
        XCTAssertTrue(sent.last?.hasPrefix("KEYINPUT ") == true, message)
    }

    private func assertNextGenerationWorks(_ session: HvfEngineSession, _ fixture: HvfRuntimeKeyboardDraftFixture,
                                           _ message: String) throws {
        let query = try fixture.commands().last ?? ""
        XCTAssertTrue(query.hasPrefix("INPUTCAPS "), "next generation negotiates: \(message)")
        guard query.hasPrefix("INPUTCAPS ") else { return }
        let id = query.split(separator: " ")[1]
        try append("BVAGENT CMD \(query) exit=0\nBVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536\n"
            + "BVAGENT END \(query)\n", fixture)
        session.poll(); session.poll()
        var draft = "new"
        XCTAssertEqual(HvfKeyboardDraft.submit(&draft, to: session), .acceptedForProcessing, message)
        let texts = try fixture.commands().filter { $0.hasPrefix("TEXTINPUT ") }
        XCTAssertEqual(texts.count, 2, "the failed generation's request is never resent: \(message)")
        XCTAssertEqual(texts.last?.split(separator: " ").last, "bmV3", message)
        XCTAssertNil(try fixture.bytes(fixture.input), message)
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
