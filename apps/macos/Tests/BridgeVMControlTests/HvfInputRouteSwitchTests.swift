import AppKit
import XCTest
@testable import BridgeVMControl

/// The session's switches between input.ctl (the VMM's HID route) and the agent's ordered route, through the
/// public send paths the display view uses: input before INPUTCAPS no longer pins the boot to HID, and a Windows
/// secure desktop (lock or sign-in screen, UAC), where SendInput inserts nothing, no longer ends input.
@MainActor
final class HvfInputRouteSwitchTests: XCTestCase {
    private let point = CGPoint(x: 50, y: 50), size = CGSize(width: 100, height: 100)
    private let active = "ordered input active: receipts confirm insertion, not application consumption"

    func testInputBeforeInputCapsNoLongerPreventsTheSwitchOnceTheHIDRouteIsQuiet() throws {
        try withClockedSession(negotiated: false) { session, fixture, clock in
            session.beginOwnedInputBoot()
            session.sendPointerMove(location: point, viewSize: size, imageSize: size)
            XCTAssertTrue(session.sendKey("enter"))
            session.poll()
            let query = try XCTUnwrap(try fixture.commands().first, "INPUTCAPS still goes out after early input")
            try answer(query, fixture); session.poll()
            XCTAssertTrue(session.sendKey("tab"), "within 1 s of the last HID key the route stays HID")
            try fixture.probeTakes(); session.poll()
            clock.advance(0.5); session.poll()
            XCTAssertTrue(session.sendKey("esc"), "each HID key restarts the quiet second")
            try fixture.probeTakes(); session.poll()
            clock.advance(0.75); session.poll()
            XCTAssertEqual(try fixture.commands(), [query])
            clock.advance(0.25); session.poll()
            XCTAssertTrue(session.events.contains(.unknown(active)))
            XCTAssertTrue(session.sendKey("left"))
            XCTAssertEqual(try payload(try fixture.commands().last), "KEYINPUT left")
            XCTAssertEqual(try legacy(fixture).filter { !$0.hasPrefix("POINTER move:") }, ["KEY enter", "KEY tab", "KEY esc"])
        }
    }

    /// A long ASCII paste is many `KEY text-hex:` lines, and the VMM takes at most one per tick and none while its
    /// xHCI queue is busy: elapsed time alone is no evidence, its run.log line for each of them is.
    func testTheSwitchWaitsUntilTheVMMHasTakenEveryHIDLine() throws {
        try withClockedSession(negotiated: false) { session, fixture, clock in
            session.beginOwnedInputBoot()
            XCTAssertEqual(session.sendText(String(repeating: "abcdefgh", count: 80)), .legacyAttempted)
            XCTAssertEqual(try legacy(fixture).filter { $0.hasPrefix("KEY text-hex:") }.count, 20)
            session.poll()
            let query = try XCTUnwrap(try fixture.commands().first)
            try answer(query, fixture); session.poll()
            try fixture.probeTakes(19)
            clock.advance(5); session.poll()
            XCTAssertFalse(session.events.contains(.unknown(active)), "one line is still queued in the VMM")
            try fixture.probeTakes(); session.poll()
            clock.advance(0.75); session.poll()
            XCTAssertFalse(session.events.contains(.unknown(active)), "within a second of the VMM taking the last one")
            clock.advance(0.25); session.poll()
            XCTAssertTrue(session.events.contains(.unknown(active)))
            XCTAssertTrue(session.sendKey("enter"))
            XCTAssertEqual(try payload(try fixture.commands().last), "KEYINPUT enter")
        }
    }

    /// A move prints no line when the VMM takes it, and a busy xHCI pointer queue keeps it queued, so time after a
    /// move is no evidence either: the session writes one no-op `POINTER release:` at the last move's position behind
    /// it, and the switch waits for the VMM to take that (only after the move's reports drained), then the second.
    func testMovesAloneSwitchOnlyOnceTheVMMHasTakenABarrierBehindThem() throws {
        try withClockedSession(negotiated: false) { session, fixture, clock in
            session.beginOwnedInputBoot()
            session.sendPointerMove(location: CGPoint(x: 10, y: 20), viewSize: size, imageSize: size)
            session.sendPointerMove(location: point, viewSize: size, imageSize: size)
            session.poll()
            let query = try XCTUnwrap(try fixture.commands().first)
            try answer(query, fixture); session.poll()
            clock.advance(5); session.poll()
            XCTAssertFalse(session.events.contains(.unknown(active)), "the VMM has not taken the barrier")
            var lines = try legacy(fixture)
            XCTAssertEqual(lines.count, 3, "one barrier at a time"); guard lines.count == 3 else { return }
            XCTAssertEqual(lines.last, barrier(after: lines[1]), "one barrier at the last move's position")
            session.sendPointerMove(location: CGPoint(x: 70, y: 80), viewSize: size, imageSize: size)
            try fixture.probeTakes(); session.poll()
            clock.advance(1); session.poll()
            XCTAssertFalse(session.events.contains(.unknown(active)), "a move after the barrier")
            lines = try legacy(fixture)
            XCTAssertEqual(lines.count, 5); guard lines.count == 5 else { return }
            XCTAssertEqual(lines.last, barrier(after: lines[3]), "a second barrier behind that move")
            try fixture.probeTakes(); session.poll()
            clock.advance(0.75); session.poll()
            XCTAssertFalse(session.events.contains(.unknown(active)), "within a second of the VMM taking it")
            clock.advance(0.25); session.poll()
            XCTAssertTrue(session.events.contains(.unknown(active)))
            session.sendPointerClick(location: point, viewSize: size, imageSize: size)
            XCTAssertEqual(try payload(try fixture.commands().last)?.prefix(18), "POINTERINPUT click")
            XCTAssertEqual(try legacy(fixture).count, 5)
        }
    }

    func testAHeldHIDButtonBlocksTheSwitch() throws {
        try withClockedSession(negotiated: false) { session, fixture, clock in
            session.beginOwnedInputBoot()
            session.sendPointerPress(location: point, viewSize: size, imageSize: size)
            try fixture.probeTakes(); session.poll()
            let query = try XCTUnwrap(try fixture.commands().first)
            try answer(query, fixture); session.poll()
            clock.advance(5); session.poll()
            session.sendPointerMove(location: point, viewSize: size, imageSize: size)
            clock.advance(5); session.poll()
            XCTAssertEqual(try fixture.commands(), [query], "no switch while the HID press is held")
            session.sendPointerRelease(location: point, viewSize: size, imageSize: size)
            try fixture.probeTakes(); session.poll()
            clock.advance(0.875); session.poll()
            XCTAssertEqual(try fixture.commands(), [query], "within a second of the VMM taking the release")
            clock.advance(0.125); session.poll()
            session.sendPointerClick(location: point, viewSize: size, imageSize: size)
            XCTAssertEqual(try payload(try fixture.commands().last)?.prefix(18), "POINTERINPUT click")
            let lines = try legacy(fixture).map { String($0.prefix { $0 != ":" }) }
            XCTAssertEqual(lines, ["POINTER press", "POINTER move", "POINTER release"],
                           "no barrier while the button is down; the release behind the move confirms it")
        }
    }

    func testNothingInsertedResendsTheEventAndItsBacklogOnHIDInOrderThenRetriesOrderedAfterQuiet() throws {
        try withClockedSession(negotiated: true) { session, fixture, clock in
            XCTAssertTrue(session.sendKey("enter"))
            let failed = try XCTUnwrap(try fixture.commands().last)
            XCTAssertEqual(session.sendText("bc"), .acceptedForProcessing)
            session.sendPointerClick(location: point, viewSize: size, imageSize: size)
            try fixture.appendLog(notInserted(failed, verb: "InsertKey")); session.poll()
            XCTAssertEqual(try fixture.commands().last, failed, "nothing more goes to the agent")
            XCTAssertTrue(session.sendKey("tab"), "later input takes the HID route")
            XCTAssertEqual(try legacy(fixture).map { String($0.prefix(18)) },
                           ["KEY enter", "KEY text-hex:6263", "POINTER click:1638", "KEY tab"])
            try fixture.probeTakes(); session.poll()
            clock.advance(1); session.poll()
            XCTAssertTrue(session.sendKey("esc"), "after a quiet second the next event tries the agent again")
            let retry = try XCTUnwrap(try fixture.commands().last)
            XCTAssertEqual(try payload(retry), "KEYINPUT esc")
            try fixture.appendLog(notInserted(retry, verb: "InsertKey")); session.poll()
            XCTAssertEqual(try legacy(fixture).last, "KEY esc", "a retry that inserts nothing goes HID again")
            XCTAssertTrue(session.sendKey("left"))
            XCTAssertEqual(try legacy(fixture).last, "KEY left")
            try fixture.probeTakes(); session.poll()
            clock.advance(1); session.poll()
            XCTAssertTrue(session.sendKey("right"))
            let back = try XCTUnwrap(try fixture.commands().last)
            XCTAssertEqual(try payload(back), "KEYINPUT right")
            try fixture.appendLog(inserted(back, count: 2)); session.poll()
            XCTAssertTrue(session.sendKey("up"))
            XCTAssertEqual(try payload(try fixture.commands().last), "KEYINPUT up", "the agent route is back")
            XCTAssertEqual(try legacy(fixture).count, 6, "no event was sent twice or dropped")
        }
    }

    func testAFallbackReplayStopsAtTextItCannotResendSoALaterEnterNeverActsOnIt() throws {
        try withClockedSession(negotiated: true) { session, fixture, _ in
            XCTAssertTrue(session.sendKey("esc"))
            let failed = try XCTUnwrap(try fixture.commands().last)
            XCTAssertEqual(try payload(failed), "KEYINPUT esc")
            // Ordered text takes 64 KiB; the legacy clipboard command stays under cmd.exe's 8191 characters.
            XCTAssertEqual(session.sendText(String(repeating: "\u{D55C}", count: 4000)), .acceptedForProcessing)
            XCTAssertTrue(session.sendKey("enter"))
            try fixture.appendLog(notInserted(failed, verb: "InsertKey")); session.poll()
            XCTAssertEqual(try legacy(fixture), ["KEY esc"], "the Enter behind the paste is not sent without it")
        }
    }

    func testAPartialInsertionKeepsTheLatchedFailure() throws {
        try withClockedSession(negotiated: true) { session, fixture, _ in
            XCTAssertTrue(session.sendKey("enter"))
            let failed = try XCTUnwrap(try fixture.commands().last)
            try fixture.appendLog(notInserted(failed, verb: "InsertKey", inserted: 1)); session.poll()
            XCTAssertFalse(session.sendKey("tab"))
            XCTAssertNil(try fixture.bytes(fixture.input), "a partly inserted key is never replayed")
        }
    }

    /// Ctrl+Alt+Delete takes the VMM keyboard, and nothing typed after it may overtake it on the agent route (an
    /// Enter or a password reaching the current app before Windows switches desktops): later input follows it on
    /// input.ctl until the VMM has taken it and input.ctl is quiet, then the ordered route is back.
    func testCtrlAltDeleteUsesTheVMMKeyboardAndLaterInputFollowsItThere() throws {
        try withClockedSession(negotiated: true) { session, fixture, clock in
            XCTAssertTrue(session.sendToolbarKey("ctrl+alt+delete"), "the menu item reports it as sent")
            XCTAssertTrue(session.sendKey("enter"))
            XCTAssertEqual(try legacy(fixture), ["KEY ctrl+alt+delete", "KEY enter"], "Enter follows the chord")
            XCTAssertEqual(try fixture.commands().count, 1, "only INPUTCAPS went to the agent")
            clock.advance(5); session.poll()
            XCTAssertTrue(session.sendKey("tab"))
            XCTAssertEqual(try legacy(fixture).last, "KEY tab", "the VMM has not taken the chord yet")
            try fixture.probeTakes(); session.poll()
            clock.advance(1); session.poll()
            XCTAssertTrue(session.sendKey("esc"))
            XCTAssertEqual(try payload(try fixture.commands().last), "KEYINPUT esc", "then the ordered route is back")
            XCTAssertEqual(try legacy(fixture).count, 3)
        }
    }

    /// Behind a key still going in on the agent route the chord waits for that key's receipt, then goes to input.ctl,
    /// and an Enter typed meanwhile follows it there instead of reaching the agent first.
    func testCtrlAltDeleteWaitsForOrderedInputAheadAndInputTypedMeanwhileFollowsIt() throws {
        try withClockedSession(negotiated: true) { session, fixture, _ in
            XCTAssertTrue(session.sendKey("tab"))
            let ahead = try XCTUnwrap(try fixture.commands().last)
            XCTAssertEqual(try payload(ahead), "KEYINPUT tab")
            XCTAssertTrue(session.sendToolbarKey("ctrl+alt+delete"), "the menu item reports it as sent")
            XCTAssertTrue(session.sendKey("enter"))
            session.poll()
            XCTAssertEqual(try legacy(fixture), [], "the chord does not overtake the key still going in")
            XCTAssertEqual(try fixture.commands().last, ahead, "the Enter never goes to the agent")
            try fixture.appendLog(inserted(ahead, count: 2)); session.poll()
            XCTAssertEqual(try legacy(fixture), ["KEY ctrl+alt+delete", "KEY enter"])
            XCTAssertEqual(try fixture.commands().last, ahead)
        }
    }

    // MARK: - Fixture

    final class Clock {
        var now = Date(timeIntervalSince1970: 50_000)
        func advance(_ seconds: TimeInterval) { now = now.addingTimeInterval(seconds) }
    }

    /// A connected fixture session on a test clock; `negotiated` also activates its ordered route.
    private func withClockedSession(negotiated: Bool,
                                    _ body: (HvfEngineSession, HvfRuntimeKeyboardDraftFixture, Clock) throws -> Void) throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        let clock = Clock()
        try fixture.withSession { session in
            session.hostNow = { clock.now }
            if negotiated { try fixture.negotiateOrderedInput(session) } else { try fixture.connect(session) }
            try body(session, fixture, clock)
        }
    }

    private func answer(_ query: String, _ fixture: HvfRuntimeKeyboardDraftFixture) throws {
        let id = query.split(separator: " ")[1]
        try fixture.appendLog("BVAGENT CMD \(query) exit=0\nBVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536\n"
            + "BVAGENT END \(query)\n")
    }

    private func label(_ command: String) -> (label: String, id: Substring) {
        let words = command.split(separator: " ")
        return (words.prefix(2).joined(separator: " "), words[1])
    }

    private func inserted(_ command: String, count: Int) -> String {
        let (label, id) = label(command)
        return "BVAGENT CMD \(label) exit=0\nBVINPUT_INSERTED \(id) \(count)\nBVAGENT END \(label)\n"
    }

    /// The guest agent's receipt when SendInput inserted fewer events than requested (bvagent-input.ps1).
    private func notInserted(_ command: String, verb: String, inserted: Int = 0) -> String {
        let (label, id) = label(command)
        return "BVAGENT CMD \(label) exit=1\nBVINPUT_FAILED \(id) Exception calling \"\(verb)\" with \"1\" argument(s): "
            + "\"unicode-input-incomplete inserted=\(inserted) requested=2\"\nBVAGENT END \(label)\n"
    }

    /// `VERB payload` of an agent command, its base64 decoded.
    private func payload(_ command: String?) throws -> String? {
        guard let words = command?.split(separator: " "), words.count == 3,
              let data = Data(base64Encoded: String(words[2])) else { return nil }
        return "\(words[0]) " + String(decoding: data, as: UTF8.self)
    }

    /// `POINTER release:` at a `POINTER move:` line's position.
    private func barrier(after move: String) -> String {
        XCTAssertTrue(move.hasPrefix("POINTER move:"), move)
        return "POINTER release:" + move.dropFirst("POINTER move:".count)
    }

    private func legacy(_ fixture: HvfRuntimeKeyboardDraftFixture) throws -> [String] {
        guard let data = try fixture.bytes(fixture.input) else { return [] }
        return String(decoding: data, as: UTF8.self).split(separator: "\n").map(String.init)
    }
}
