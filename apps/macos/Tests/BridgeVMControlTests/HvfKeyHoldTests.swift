import AppKit
import XCTest
@testable import BridgeVMControl

/// A named key goes down at the Mac's key-down and up at its key-up on both routes, through the display's key path
/// (HvfFramebufferKeyInput): the VMM keeps it in its HID report and Windows repeats it, or the agent presses it with
/// SendInput and each Mac repeat is one more key-down. Every key goes up on the route it went down, and nothing stays
/// down after focus loss, a route switch, a fallback, Ctrl+Alt+Delete or an agent restart.
@MainActor
final class HvfKeyHoldTests: XCTestCase {
    private let active = "ordered input active: receipts confirm insertion, not application consumption"
    private struct Key { let characters: String, keyCode: UInt16 }
    private let left = Key(characters: "\u{f702}", keyCode: 0x7b)
    private let right = Key(characters: "\u{f703}", keyCode: 0x7c)

    func testTheHIDRouteHoldsAKeyFromItsKeyDownToItsKeyUpAndLeavesRepeatsToWindows() throws {
        try withClockedSession(hold: nil) { session, fixture, _ in
            try press(left, session)
            for _ in 0..<3 { try press(left, session, repeat: true) }
            try type("a", session)
            try release(left, session)
            XCTAssertEqual(try legacy(fixture), ["KEY down:left", "KEY text-hex:61", "KEY up:left"],
                           "one continuous press; Windows repeats it, text typed meanwhile is a tap")
            // Shift comes down under the held key: its next repeat is another key.
            try press(left, session)
            try press(left, session, repeat: true, flags: .shift)
            try press(left, session, repeat: true, flags: .shift)
            try release(left, session, flags: .shift)
            XCTAssertEqual(try legacy(fixture).dropFirst(3), ["KEY down:left", "KEY up:left", "KEY down:shift+left",
                                                              "KEY up:shift+left"])
        }
    }

    func testAtMostSixKeysAreHeldAndASeventhIsATap() throws {
        try withClockedSession(hold: nil) { session, fixture, _ in
            let codes: [UInt16] = [0x7a, 0x78, 0x63, 0x76, 0x60, 0x61, 0x62] // F1 to F7
            let keys = codes.enumerated().map { index, code in
                Key(characters: String(Character(UnicodeScalar(UInt32(0xf704 + index))!)), keyCode: code)
            }
            for key in keys { try press(key, session) }
            XCTAssertEqual(try legacy(fixture), (1...6).map { "KEY down:f\($0)" } + ["KEY f7"])
            for key in keys { try release(key, session) }
            XCTAssertEqual(Array(try legacy(fixture).dropFirst(7)), (1...6).map { "KEY up:f\($0)" })
        }
    }

    func testTheOrderedRouteHoldsAKeyAndForwardsMacRepeatsAsKeyDownsNeverExtraKeyUps() throws {
        try withClockedSession(hold: true) { session, fixture, _ in
            try press(left, session)
            try insert(fixture, session, count: 1)
            try press(left, session, repeat: true)
            try insert(fixture, session, count: 1)
            try press(left, session, repeat: true)
            try press(left, session, repeat: true)
            try press(left, session, repeat: true) // absorbed: one already waits behind the one in flight
            try insert(fixture, session, count: 1)
            try insert(fixture, session, count: 1)
            try release(left, session)
            try insert(fixture, session, count: 1)
            XCTAssertEqual(try payloads(fixture), ["KEYINPUT down:left", "KEYINPUT down:left", "KEYINPUT down:left",
                                                   "KEYINPUT down:left", "KEYINPUT up:left"])
            XCTAssertNil(try fixture.bytes(fixture.input))
        }
    }

    func testAnAgentWithoutTheHoldMarkerGetsTapsAsBefore() throws {
        try withClockedSession(hold: false) { session, fixture, _ in
            try press(left, session)
            try insert(fixture, session, count: 2)
            try press(left, session, repeat: true)
            try insert(fixture, session, count: 2)
            try release(left, session)
            session.poll()
            XCTAssertEqual(try payloads(fixture), ["KEYINPUT left", "KEYINPUT left"], "a key-up sends nothing")
            XCTAssertNil(try fixture.bytes(fixture.input))
        }
    }

    func testFocusLossReleasesEveryHeldKeyOnTheRouteItWentDown() throws {
        try withClockedSession(hold: nil) { session, fixture, _ in
            try press(left, session)
            try press(right, session, flags: .control)
            session.cancelOrderedInputTarget() // the display window lost key focus
            try release(left, session)
            try release(right, session, flags: .control)
            XCTAssertEqual(try legacy(fixture), ["KEY down:left", "KEY down:ctrl+right", "KEY up:left", "KEY up:ctrl+right"],
                           "input.ctl's keys go up there at once, and their later key-ups send nothing")
        }
        try withClockedSession(hold: true) { session, fixture, _ in
            try press(left, session)
            try insert(fixture, session, count: 1)
            session.cancelOrderedInputTarget()
            session.poll()
            XCTAssertEqual(try payloads(fixture).last, "KEYINPUT up:left", "the agent's go up through the cleanup")
            try insert(fixture, session, count: 1)
            try release(left, session)
            session.poll()
            XCTAssertEqual(try payloads(fixture), ["KEYINPUT down:left", "KEYINPUT up:left"])
            XCTAssertNil(try fixture.bytes(fixture.input))
        }
    }

    /// The switch to the agent route waits while input.ctl holds a key (its release must go there: the HID keyboard
    /// cannot release a SendInput press, nor SendInput an HID one), then the next key goes to the agent.
    func testTheSwitchWaitsForAKeyHeldOnHIDAndItsReleaseGoesThere() throws {
        try withClockedSession(hold: nil) { session, fixture, clock in
            session.beginOwnedInputBoot()
            try press(left, session)
            session.poll()
            let query = try XCTUnwrap(try fixture.commands().first)
            try answer(query, fixture, hold: true); session.poll()
            try fixture.probeTakes(); session.poll()
            clock.advance(5); session.poll()
            XCTAssertFalse(session.events.contains(.unknown(active)), "no switch while the HID key is down")
            try release(left, session)
            XCTAssertEqual(try legacy(fixture), ["KEY down:left", "KEY up:left"])
            try fixture.probeTakes(); session.poll()
            clock.advance(1); session.poll()
            XCTAssertTrue(session.events.contains(.unknown(active)))
            try press(left, session)
            XCTAssertEqual(try payloads(fixture), ["KEYINPUT down:left"])
        }
    }

    /// A key the agent pressed before a secure desktop appeared (a later event inserted nothing) stays owed to the agent
    /// route: its key-up sends nothing on input.ctl, and leaving the fallback sends its release before anything else.
    func testAFallbackLeavesAKeyTheAgentPressedToTheAgentsRelease() throws {
        try withClockedSession(hold: true) { session, fixture, clock in
            try press(left, session)
            try insert(fixture, session, count: 1)
            XCTAssertEqual(session.sendText("b"), .acceptedForProcessing)
            let text = try XCTUnwrap(try fixture.commands().last)
            try fixture.appendLog(notInserted(text)); session.poll()
            try release(left, session)
            XCTAssertEqual(try legacy(fixture), ["KEY text-hex:62"], "the key-up goes nowhere near input.ctl")
            try fixture.probeTakes(); session.poll()
            clock.advance(1); session.poll()
            XCTAssertEqual(try payloads(fixture).last, "KEYINPUT up:left", "leaving the fallback releases it first")
            try insert(fixture, session, count: 1); session.poll()
            try press(right, session)
            XCTAssertEqual(try payloads(fixture), ["KEYINPUT down:left", "TEXTINPUT b", "KEYINPUT up:left",
                                                   "KEYINPUT down:right"])
        }
    }

    func testCtrlAltDeleteReleasesHeldKeysOnTheirRouteFirst() throws {
        try withClockedSession(hold: nil) { session, fixture, _ in
            try press(left, session)
            XCTAssertTrue(session.sendToolbarKey("ctrl+alt+delete"))
            try release(left, session)
            XCTAssertEqual(try legacy(fixture), ["KEY down:left", "KEY up:left", "KEY ctrl+alt+delete"])
        }
        try withClockedSession(hold: true) { session, fixture, _ in
            try press(left, session)
            try insert(fixture, session, count: 1)
            XCTAssertTrue(session.sendToolbarKey("ctrl+alt+delete"))
            XCTAssertEqual(try payloads(fixture).last, "KEYINPUT up:left")
            XCTAssertNil(try fixture.bytes(fixture.input), "the chord waits for the release")
            try insert(fixture, session, count: 1)
            XCTAssertEqual(try legacy(fixture), ["KEY ctrl+alt+delete"])
        }
    }

    /// Windows keeps a key the old agent instance pressed down (SendInput's state is not per process): the restarted
    /// agent gets its release ahead of the new INPUTCAPS, and the later key-up sends nothing twice.
    func testAnAgentRestartSendsTheKeysTheOldInstancePressedUpToTheNewOne() throws {
        try withClockedSession(hold: true) { session, fixture, _ in
            try press(left, session)
            try insert(fixture, session, count: 1)
            try fixture.appendLog("BVAGENT re-READY BRIDGEVM t=40\n"); session.poll()
            session.poll()
            try release(left, session)
            let commands = try fixture.commands().dropFirst(2)
            XCTAssertEqual(commands.map { (try? payload($0)) ?? String($0.prefix(9)) }, ["KEYINPUT up:left", "INPUTCAPS"])
            XCTAssertNil(try fixture.bytes(fixture.input))
        }
    }

    /// The pieces: a held key's halves insert half of its tap's events, only an agent with the marker gets them, a sent
    /// press stays owed until its release is inserted, and input.ctl is not quiet while it holds a key.
    func testHeldKeyHalvesTheirMarkerAndTheStateThatOwesTheirRelease() throws {
        XCTAssertEqual(HvfGuestInputEncoding(.key("down:left"))?.insertedEventCount, 1)
        XCTAssertEqual(HvfGuestInputEncoding(.key("up:ctrl+shift+left"))?.insertedEventCount, 3)
        XCTAssertEqual(HvfGuestInputEncoding(.key("ctrl+left"))?.insertedEventCount, 4)
        for invalid in ["down:", "down:a", "down:ctrl+alt+delete", "up:down:left", "hold:left"] {
            XCTAssertNil(HvfGuestInputEncoding(.key(invalid)), invalid)
        }
        let id = UUID()
        for hold in [false, true] {
            var receipt = HvfInputCapabilityReceipt(id: id)
            for line in ["BVAGENT CMD INPUTCAPS \(id.uuidString) exit=0", "BVINPUT_CAPS \(id.uuidString) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536",
                         "BVINPUT_CHORDS \(id.uuidString) MODIFIER_CHORDS"] + (hold ? ["BVINPUT_KEYS \(id.uuidString) HOLD"] : []) {
                XCTAssertNil(receipt.consume(line))
            }
            XCTAssertEqual(receipt.supports(.key("down:ctrl+s")), hold)
            XCTAssertEqual(receipt.supports(.key("up:left")), hold)
            XCTAssertTrue(receipt.supports(.key("ctrl+s")))
        }

        var held = HvfPointerHeldState()
        held.sent(.key("down:left"))
        held.sent(.key("down:left")) // a Mac repeat
        held.sent(.pointer("press:3x4"))
        XCTAssertEqual(held.releases, [.key("up:left"), .pointer("releaseall:3x4")])
        held.inserted(.key("up:left"))
        XCTAssertEqual(held.releases, [.pointer("releaseall:3x4")])

        var activity = HvfLegacyInputActivity()
        let start = Date(timeIntervalSince1970: 1000)
        activity.wrote("KEY down:left", now: start)
        activity.observe(["live input accepted: command=Key(<redacted>)"], now: start)
        XCTAssertFalse(activity.quiescent(now: start.addingTimeInterval(10)), "a key input.ctl holds blocks the switch")
        activity.wrote("KEY up:left", now: start)
        activity.observe(["live input accepted: command=Key(<redacted>)"], now: start)
        XCTAssertTrue(activity.quiescent(now: start.addingTimeInterval(1)))
        activity.wrote("KEY down:left", now: start)
        activity.observe(["live input accepted: command=Key(<redacted>)", HvfGuestResetRecord.processRecreation], now: start)
        XCTAssertEqual(activity.keys, [], "a recreated VMM holds nothing")
    }

    // MARK: - Fixture

    final class Clock {
        var now = Date(timeIntervalSince1970: 50_000)
        func advance(_ seconds: TimeInterval) { now = now.addingTimeInterval(seconds) }
    }

    /// `hold`: nil keeps the HID route (no owned boot); true or false activates the agent route with or without the
    /// agent's HOLD marker (and with its generic chords).
    private func withClockedSession(hold: Bool?,
                                    _ body: (HvfEngineSession, HvfRuntimeKeyboardDraftFixture, Clock) throws -> Void) throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        let clock = Clock()
        try fixture.withSession { session in
            session.hostNow = { clock.now }
            try fixture.connect(session)
            if let hold {
                session.beginOwnedInputBoot()
                session.poll()
                let query = try XCTUnwrap(try fixture.commands().first)
                try answer(query, fixture, hold: hold)
                session.poll(); session.poll()
                XCTAssertTrue(session.events.contains(.unknown(active)))
            }
            try body(session, fixture, clock)
        }
    }

    private func answer(_ query: String, _ fixture: HvfRuntimeKeyboardDraftFixture, hold: Bool) throws {
        let id = String(query.split(separator: " ")[1])
        try fixture.appendLog("BVAGENT CMD \(query) exit=0\nBVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536\n"
            + "BVINPUT_CHORDS \(id) MODIFIER_CHORDS\n" + (hold ? "BVINPUT_KEYS \(id) HOLD\n" : "") + "BVAGENT END \(query)\n")
    }

    private func event(_ type: NSEvent.EventType, _ key: Key, repeat isRepeat: Bool = false,
                       flags: NSEvent.ModifierFlags = []) throws -> NSEvent {
        try XCTUnwrap(NSEvent.keyEvent(with: type, location: .zero, modifierFlags: flags, timestamp: 0, windowNumber: 0,
                                       context: nil, characters: key.characters, charactersIgnoringModifiers: key.characters,
                                       isARepeat: isRepeat, keyCode: key.keyCode))
    }

    private func press(_ key: Key, _ session: HvfEngineSession, repeat isRepeat: Bool = false,
                       flags: NSEvent.ModifierFlags = []) throws {
        var refused = false
        XCTAssertTrue(HvfFramebufferKeyInput.send(try event(.keyDown, key, repeat: isRepeat, flags: flags), to: session) { refused = true })
        XCTAssertFalse(refused)
    }

    private func release(_ key: Key, _ session: HvfEngineSession,
                         flags: NSEvent.ModifierFlags = []) throws {
        _ = HvfFramebufferKeyInput.send(try event(.keyUp, key, flags: flags), to: session) { XCTFail("a key-up is never refused") }
    }

    private func type(_ text: String, _ session: HvfEngineSession) throws {
        XCTAssertTrue(HvfFramebufferKeyInput.send(try event(.keyDown, Key(characters: text, keyCode: 0x00)), to: session))
    }

    /// The agent's receipt for the request it got last.
    private func insert(_ fixture: HvfRuntimeKeyboardDraftFixture, _ session: HvfEngineSession, count: Int) throws {
        let command = try XCTUnwrap(try fixture.commands().last)
        let words = command.split(separator: " "), label = words.prefix(2).joined(separator: " ")
        try fixture.appendLog("BVAGENT CMD \(label) exit=0\nBVINPUT_INSERTED \(words[1]) \(count)\nBVAGENT END \(label)\n")
        session.poll()
    }

    /// SendInput inserted nothing (a lock, sign-in or UAC screen; bvagent-input.ps1).
    private func notInserted(_ command: String) -> String {
        let words = command.split(separator: " "), label = words.prefix(2).joined(separator: " ")
        return "BVAGENT CMD \(label) exit=1\nBVINPUT_FAILED \(words[1]) Exception calling \"Insert\" with \"1\" argument(s): "
            + "\"unicode-input-incomplete inserted=0 requested=2\"\nBVAGENT END \(label)\n"
    }

    /// `VERB payload` of an agent command, its base64 decoded.
    private func payload(_ command: String) throws -> String? {
        let words = command.split(separator: " ")
        guard words.count == 3, let data = Data(base64Encoded: String(words[2])) else { return nil }
        return "\(words[0]) " + String(decoding: data, as: UTF8.self)
    }

    /// The agent's input requests, INPUTCAPS left out.
    private func payloads(_ fixture: HvfRuntimeKeyboardDraftFixture) throws -> [String] {
        try fixture.commands().compactMap { try payload($0) }
    }

    private func legacy(_ fixture: HvfRuntimeKeyboardDraftFixture) throws -> [String] {
        guard let data = try fixture.bytes(fixture.input) else { return [] }
        return String(decoding: data, as: UTF8.self).split(separator: "\n").map(String.init)
    }
}
