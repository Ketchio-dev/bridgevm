import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfSessionInputRouterTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 2_000)
    private let binding = ["disk", "vars", "evidence", "control"]

    private func negotiate(_ router: inout HvfSessionInputRouter, at time: Date? = nil) {
        let time = time ?? now
        var command = ""
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: time) { command = $0; return true }
        let id = command.split(separator: " ").last ?? "missing"
        let lines = ["BVAGENT CMD \(command) exit=0", "BVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536",
                     "BVAGENT END \(command)"]
        _ = router.poll(binding: binding, serviceReady: true, lines: lines, now: time) { _ in false }
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: time) { _ in false }
    }

    private func poll(_ router: inout HvfSessionInputRouter, at seconds: TimeInterval, lines: [String] = [],
                      sent: ((String) -> Void)? = nil) -> HvfAcknowledgedInputStream.Update? {
        router.poll(binding: binding, serviceReady: true, lines: lines, now: now.addingTimeInterval(seconds)) {
            if let sent { sent($0); return true }
            XCTFail("unexpected send \($0)"); return false
        }
    }

    private func wrote(_ router: inout HvfSessionInputRouter, _ line: String, at seconds: TimeInterval = 0) {
        XCTAssertTrue(router.allowLegacyWrite(line, binding: binding, now: now.addingTimeInterval(seconds)), line)
    }

    /// bvagent-input.ps1's receipt when SendInput inserted `inserted` of 2 events.
    private func shortfall(_ command: String, inserted: Int = 0) -> [String] {
        let words = command.split(separator: " "), label = words.prefix(2).joined(separator: " ")
        guard words.count >= 2 else { XCTFail("no command sent"); return [] }
        return ["BVAGENT CMD \(label) exit=1", "BVINPUT_FAILED \(words[1]) Exception calling \"InsertPointer\" with \"1\" "
                + "argument(s): \"unicode-input-incomplete inserted=\(inserted) requested=2\"", "BVAGENT END \(label)"]
    }

    func testOwnedUntaintedBootCanActivateButUnknownAttachmentCannot() {
        var router = HvfSessionInputRouter()
        router.attachUnknown(binding: binding)
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) {
            _ in XCTFail("unknown history negotiated"); return true
        }
        XCTAssertEqual(router.route(.text("a"), binding: binding, now: now), .legacy)
        router.beginOwnedBoot(binding: binding)
        negotiate(&router)
        XCTAssertEqual(router.state, .ready)
        XCTAssertEqual(router.route(.text("a"), binding: binding, now: now), .queued)
        XCTAssertFalse(router.allowLegacyWrite("KEY enter", binding: binding, now: now))
    }

    func testLegacyInputBeforeActivationOnlyDelaysTheSwitchUntilTheHIDRouteIsQuiet() {
        for directWrite in [false, true] {
            var router = HvfSessionInputRouter()
            router.beginOwnedBoot(binding: binding)
            if !directWrite { XCTAssertEqual(router.route(.key("enter"), binding: binding, now: now), .legacy) }
            wrote(&router, directWrite ? "KEY ctrl+v" : "KEY enter")
            negotiate(&router)
            XCTAssertEqual(router.state, .waitingForLegacy, "INPUTCAPS still went out")
            _ = router.cancelTarget(now: now)
            _ = poll(&router, at: 0.25)
            XCTAssertEqual(router.state, .waitingForLegacy, "a target change keeps the agent's answer, no new INPUTCAPS")
            XCTAssertEqual(router.route(.pointer("click:0x0"), binding: binding, now: now), .legacy)
            wrote(&router, "POINTER click:0x0", at: 0.5)
            _ = poll(&router, at: 1.25)
            XCTAssertEqual(router.state, .waitingForLegacy, "within a second of the last HID click")
            XCTAssertFalse(router.activated)
            _ = poll(&router, at: 1.5)
            XCTAssertEqual(router.state, .ready)
            XCTAssertTrue(router.activated)
            XCTAssertEqual(router.route(.pointer("click:0x0"), binding: binding, now: now), .queued)
            XCTAssertFalse(router.allowLegacyWrite("KEY enter", binding: binding, now: now), "the active route never mixes")
        }
    }

    func testAHeldHIDButtonBlocksTheSwitchAndAMoveSettlesInAQuarterSecond() {
        var router = HvfSessionInputRouter()
        router.beginOwnedBoot(binding: binding)
        wrote(&router, "POINTER right-press:1x2")
        negotiate(&router)
        wrote(&router, "POINTER click:1x2", at: 1)
        _ = poll(&router, at: 60)
        XCTAssertEqual(router.state, .waitingForLegacy, "a left click leaves the right button down")
        wrote(&router, "POINTER release:3x4", at: 60)
        wrote(&router, "POINTER move:5x6", at: 60.875)
        _ = poll(&router, at: 61)
        XCTAssertEqual(router.state, .waitingForLegacy, "a move an eighth of a second ago")
        _ = poll(&router, at: 61.125)
        XCTAssertEqual(router.state, .ready)

        var moved = HvfSessionInputRouter()
        moved.beginOwnedBoot(binding: binding)
        wrote(&moved, "POINTER move:1x2")
        negotiate(&moved)
        XCTAssertEqual(moved.state, .waitingForLegacy)
        _ = poll(&moved, at: 0.25)
        XCTAssertEqual(moved.state, .ready, "moves alone need no quiet second")
    }

    func testNothingInsertedHandsTheEventAndItsBacklogBackInOrderAndRetriesOrderedOnceQuiet() throws {
        var router = HvfSessionInputRouter()
        router.beginOwnedBoot(binding: binding)
        negotiate(&router)
        XCTAssertEqual(router.route(.pointer("press:1x2"), binding: binding, now: now), .queued)
        var press = ""
        _ = poll(&router, at: 0) { press = $0 }
        for event: HvfOrderedInputQueue.Event in [.text("ab"), .pointer("move:3x4"), .pointer("move:5x6"),
                                                  .pointer("releaseall:5x6")] {
            XCTAssertEqual(router.route(event, binding: binding, now: now), .queued)
        }
        XCTAssertEqual(poll(&router, at: 0.1, lines: shortfall(press)), .cancelled(.notInserted, discarded: 0))
        XCTAssertEqual(router.takeLegacyReplay(), [.pointer("press:1x2"), .text("ab"), .pointer("move:5x6"),
                                                   .pointer("releaseall:5x6")], "the coalesced move once, in order")
        XCTAssertEqual(router.takeLegacyReplay(), [])
        XCTAssertFalse(router.failed)
        XCTAssertTrue(router.fallback)
        XCTAssertEqual(router.count, 0)
        XCTAssertEqual(router.state, .ready, "the agent's negotiated encodings stay")
        XCTAssertFalse(router.scrollUnits || router.pointerModifiers || router.middleButton)
        XCTAssertTrue(router.modifierChords)
        for (line, at) in [("POINTER press:1x2", 0.1), ("KEY text-hex:6162", 0.1), ("POINTER move:5x6", 0.1),
                           ("POINTER release:5x6", 0.1)] { wrote(&router, line, at: at) }
        XCTAssertEqual(router.route(.key("enter"), binding: binding, now: now), .legacy)
        wrote(&router, "KEY enter", at: 0.5)
        XCTAssertEqual(router.cancelTarget(now: now), .cancelled(.targetChanged, discarded: 0))
        XCTAssertEqual(router.count, 0, "the press inserted nothing, so the agent owes no release")
        _ = poll(&router, at: 1.25)
        XCTAssertTrue(router.fallback)
        _ = poll(&router, at: 1.5)
        XCTAssertFalse(router.fallback, "a quiet second later the next event tries the agent again")
        XCTAssertEqual(router.route(.key("tab"), binding: binding, now: now), .queued)
        var retry = ""
        _ = poll(&router, at: 1.5) { retry = $0 }
        _ = poll(&router, at: 1.6, lines: shortfall(retry))
        XCTAssertEqual(router.takeLegacyReplay(), [.key("tab")])
        XCTAssertTrue(router.fallback)
        XCTAssertEqual(router.route(.key("esc"), binding: binding, now: now), .legacy)
    }

    func testAPartlyInsertedEventStillFailsTheRouteWithoutAReplay() {
        var router = HvfSessionInputRouter()
        router.beginOwnedBoot(binding: binding)
        negotiate(&router)
        XCTAssertEqual(router.route(.pointer("click:1x2"), binding: binding, now: now), .queued)
        var click = ""
        _ = poll(&router, at: 0) { click = $0 }
        XCTAssertEqual(router.route(.key("enter"), binding: binding, now: now), .queued)
        XCTAssertEqual(poll(&router, at: 0.1, lines: shortfall(click, inserted: 1)), .cancelled(.transportFailed, discarded: 2))
        XCTAssertTrue(router.failed)
        XCTAssertEqual(router.takeLegacyReplay(), [])
        XCTAssertEqual(router.route(.key("enter"), binding: binding, now: now), .refused)
    }

    func testCtrlAltDeleteTakesTheVMMKeyboardWhateverTheRoute() {
        var router = HvfSessionInputRouter()
        router.beginOwnedBoot(binding: binding)
        negotiate(&router)
        XCTAssertEqual(router.route(.text("queued"), binding: binding, now: now), .queued)
        for key in ["ctrl+alt+delete", "ctrl+alt+shift+delete"] {
            XCTAssertEqual(router.route(.key(key), binding: binding, now: now), .legacy, key)
            wrote(&router, "KEY " + key)
        }
        XCTAssertEqual(router.count, 1, "the ordered queue is untouched")
        XCTAssertFalse(router.fallback)
        XCTAssertFalse(router.allowLegacyWrite("KEY enter", binding: binding, now: now))
        _ = poll(&router, at: 0) { _ in }
        XCTAssertEqual(poll(&router, at: 0), .waiting, "the sent text is still awaited, not failed")
        XCTAssertEqual(router.cancelTarget(now: now), .cancelled(.targetChanged, discarded: 1))
        _ = router.poll(binding: binding, serviceReady: false, lines: [], now: now) { _ in false }
        XCTAssertTrue(router.failed)
        XCTAssertEqual(router.route(.key("ctrl+alt+delete"), binding: binding, now: now), .legacy, "a failed route too")
        wrote(&router, "KEY ctrl+alt+delete")
        XCTAssertEqual(router.route(.key("ctrl+alt+delete"), binding: ["other"], now: now), .refused, "never another VM's")
    }

    func testTargetChangeCancelsPendingInputKeepsNegotiationAndNeverFallsBackToHID() {
        var router = HvfSessionInputRouter()
        router.beginOwnedBoot(binding: binding)
        negotiate(&router)
        XCTAssertEqual(router.route(.text("old target"), binding: binding, now: now), .queued)
        XCTAssertEqual(router.cancelTarget(), .cancelled(.targetChanged, discarded: 1))
        XCTAssertFalse(router.allowLegacyWrite("KEY enter", binding: binding, now: now))
        XCTAssertEqual(router.route(.pointer("click:1x2"), binding: binding, now: now), .queued, "the focusing click")
        var sent = ""; _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { sent = $0; return true }
        XCTAssertTrue(sent.hasPrefix("POINTERINPUT "), "no renegotiation for a host target change")
    }

    func testPollAloneLatchesRestartAndBindingChangeUntilFreshOwnedBoot() {
        for changedBinding in [false, true] {
            var router = HvfSessionInputRouter()
            router.beginOwnedBoot(binding: binding)
            negotiate(&router)
            XCTAssertEqual(router.route(.text("pending"), binding: binding, now: now), .queued)
            _ = router.poll(binding: changedBinding ? ["other"] : binding, serviceReady: true,
                            lines: changedBinding ? [] : ["PSCI SYSTEM_RESET: reboot 1/8"], now: now) {
                _ in XCTFail("sent across changed ownership"); return true
            }
            XCTAssertTrue(router.failed)
            XCTAssertEqual(router.count, 0)
            XCTAssertEqual(router.route(.key("enter"), binding: binding, now: now), .refused)
            router.beginOwnedBoot(binding: binding)
            negotiate(&router)
            XCTAssertEqual(router.route(.key("enter"), binding: binding, now: now), .queued)
        }
    }

    func testTransportFailureDoesNotFallBackOrReplay() {
        var router = HvfSessionInputRouter()
        router.beginOwnedBoot(binding: binding)
        negotiate(&router)
        XCTAssertEqual(router.route(.pointer("click:0x0"), binding: binding, now: now), .queued)
        XCTAssertEqual(router.poll(binding: binding, serviceReady: true, lines: [], now: now) { _ in false },
                       .cancelled(.transportFailed, discarded: 1))
        XCTAssertTrue(router.failed)
        XCTAssertEqual(router.route(.text("do not replay"), binding: binding, now: now), .refused)
        XCTAssertFalse(router.allowLegacyWrite("KEY enter", binding: binding, now: now))
    }
}
