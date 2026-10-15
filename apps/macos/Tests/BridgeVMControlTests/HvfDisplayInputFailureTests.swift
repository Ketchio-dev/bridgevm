import Foundation
import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfDisplayInputFailureTests: XCTestCase {
    func testUnwritableLegacyChannelRefusesDisplayKeyAndText() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try FileManager.default.createDirectory(at: fixture.input, withIntermediateDirectories: false)
            XCTAssertFalse(session.sendDisplayInput(.key("enter")))
            XCTAssertFalse(session.sendDisplayInput(.text("ASCII")))
            XCTAssertFalse(session.sendDisplayInput(.text("")))
            XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: fixture.input.path).isEmpty)
            XCTAssertNil(try fixture.bytes(fixture.control))
            XCTAssertTrue(session.events.contains { $0.displayText.contains("live input write failed:") })
        }
    }

    func testReplacedLegacyChannelRefusesInputWithoutChangingEitherFile() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            XCTAssertTrue(session.sendDisplayInput(.key("esc")))
            let retired = fixture.root.appendingPathComponent("retired-input")
            try FileManager.default.moveItem(at: fixture.input, to: retired)
            let original = try Data(contentsOf: retired)
            let replacement = Data("replacement".utf8)
            try replacement.write(to: fixture.input)
            XCTAssertFalse(session.sendDisplayInput(.key("enter")))
            XCTAssertFalse(session.sendDisplayInput(.text("ASCII")))
            XCTAssertEqual(try Data(contentsOf: retired), original)
            XCTAssertEqual(try fixture.bytes(fixture.input), replacement)
        }
    }

    func testFailedReplayedKeyStopsBeforeLaterClipboardText() throws {
        try assertReplayStops(after: .key("enter"))
    }

    func testFailedReplayedASCIIStopsBeforeLaterClipboardText() throws {
        try assertReplayStops(after: .text("ASCII"))
    }

    func testFullOrderedQueueRefusesDisplayInputWithoutLegacyFallback() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session)
            for _ in 0..<HvfOrderedInputQueue.maximumEvents {
                XCTAssertTrue(session.sendDisplayInput(.key("enter")))
            }
            let before = try fixture.commands()
            XCTAssertEqual(before.count, 2)
            XCTAssertTrue(try XCTUnwrap(before.last).hasPrefix("KEYINPUT "))
            XCTAssertFalse(session.sendDisplayInput(.key("backspace")))
            XCTAssertFalse(session.sendDisplayInput(.text("refused")))
            XCTAssertEqual(try fixture.commands(), before)
            XCTAssertNil(try fixture.bytes(fixture.input))
        }
    }

    func testSuccessfulLegacyDisplayInputWritesEveryASCIIChunk() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            XCTAssertTrue(session.sendDisplayInput(.text(String(repeating: "a", count: 65))))
            XCTAssertTrue(session.sendDisplayInput(.key("enter")))
            let chunk = "KEY text-hex:" + String(repeating: "61", count: 32) + "\n"
            XCTAssertEqual(try fixture.bytes(fixture.input), Data((chunk + chunk + "KEY text-hex:61\nKEY enter\n").utf8))
            XCTAssertNil(try fixture.bytes(fixture.control))
        }
    }

    func testKeyboardFailureInvokesRefusalFeedbackAndToolbarRejects() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try FileManager.default.createDirectory(at: fixture.input, withIntermediateDirectories: false)
            var refusals = 0
            for (text, keyCode) in [("\r", UInt16(36)), ("a", UInt16(0))] {
                let event = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero,
                    modifierFlags: [], timestamp: 0, windowNumber: 0, context: nil,
                    characters: text, charactersIgnoringModifiers: text, isARepeat: false, keyCode: keyCode))
                XCTAssertTrue(HvfFramebufferKeyInput.send(event, to: session, refused: { refusals += 1 }))
            }
            XCTAssertEqual(refusals, 2, "Recognized keys must still signal local submission failure")
            XCTAssertFalse(session.sendToolbarKey("tab"))
            XCTAssertNil(try fixture.bytes(fixture.control))
        }
    }

    private func assertReplayStops(after input: HvfLegacyPasteState.Input) throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.connect(session)
            try FileManager.default.createDirectory(at: fixture.input, withIntermediateDirectories: false)
            session.replayDisplayInput([input, .text("한글"), .key("backspace")])
            XCTAssertNil(session.legacyPaste.pending)
            XCTAssertEqual(session.legacyPaste.backlog, [])
            XCTAssertNil(try fixture.bytes(fixture.control), "A refused event must stop the later Set-Clipboard")
            XCTAssertTrue(session.events.contains(.unknown("display input replay stopped; discarded 2 held display input entries")))
            XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: fixture.input.path).isEmpty)
        }
    }
}
