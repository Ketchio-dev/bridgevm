#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

/// Live-display keys typed while a legacy clipboard paste is pending have no draft to stay in:
/// they must wait for that paste's `KEY ctrl+v` and then reach the guest in order.
@MainActor
final class HvfLegacyPasteBacklogTests: XCTestCase, HvfLegacyPasteFixture {
    private func type(_ keys: [String], into session: HvfEngineSession, refused: () -> Void = { XCTFail("beep") }) throws {
        for characters in keys {
            let event = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [],
                timestamp: 0, windowNumber: 0, context: nil, characters: characters,
                charactersIgnoringModifiers: characters, isARepeat: false, keyCode: 0))
            XCTAssertTrue(HvfFramebufferKeyInput.send(event, to: session, refused: refused), characters)
        }
    }

    private func input(_ root: URL) -> String {
        (try? String(contentsOf: root.appendingPathComponent("input.ctl"), encoding: .utf8)) ?? ""
    }

    private func commands(_ session: HvfEngineSession) throws -> [String] {
        try String(contentsOfFile: session.config.ctlFilePath, encoding: .utf8).split(separator: "\n").map(String.init)
    }

    private func refusals(_ session: HvfEngineSession) -> [String] {
        session.events.map(\.displayText).filter { $0.contains("refused") }
    }

    private func pasted(_ text: String) -> String { "FromBase64String('\(Data(text.utf8).base64EncodedString())')" }

    private func appendLog(_ text: String, _ root: URL) throws {
        let handle = try FileHandle(forWritingTo: root.appendingPathComponent("run.log"))
        defer { try? handle.close() }
        try handle.seekToEnd(); try handle.write(contentsOf: Data(text.utf8))
    }

    func testDisplayInputBehindLegacyPasteReplaysInOrderAfterItsCtrlV() throws {
        try withSession { session, root in
            try type(["한"], into: session)
            let first = try lastCommand(session)
            try type(["글", " ", "a", "\u{7f}", "b", "\r"], into: session)
            XCTAssertEqual(refusals(session), [], "Display input must wait, not be refused")
            XCTAssertEqual(try commands(session), [first])
            XCTAssertEqual(input(root), "", "Nothing may overtake the pending paste")
            var draft = "초안"
            XCTAssertEqual(HvfKeyboardDraft.submit(&draft, to: session), .refused)
            XCTAssertEqual(draft, "초안", "The draft box still refuses and keeps its text")

            try acknowledge(first, root: root); session.poll()
            XCTAssertEqual(input(root), "KEY ctrl+v\n")
            let second = try lastCommand(session)
            XCTAssertEqual(try commands(session), [first, second])
            XCTAssertTrue(second.contains(pasted("글 a")), "Adjacent display text merges into one paste")
            try type(["다"], into: session)

            try acknowledge(second, root: root); session.poll()
            XCTAssertEqual(input(root), "KEY ctrl+v\nKEY ctrl+v\nKEY backspace\nKEY text-hex:62\nKEY enter\n")
            let third = try lastCommand(session)
            XCTAssertEqual(try commands(session), [first, second, third])
            XCTAssertTrue(third.contains(pasted("다")))
            try acknowledge(third, root: root); session.poll()
            XCTAssertEqual(input(root), "KEY ctrl+v\nKEY ctrl+v\nKEY backspace\nKEY text-hex:62\nKEY enter\nKEY ctrl+v\n")
            XCTAssertEqual(refusals(session), ["text input refused: clipboard paste pending"], "Only the draft refusal")
        }
    }

    func testEveryPasteCancellationDiscardsHeldDisplayInput() throws {
        let size = CGSize(width: 100, height: 100)
        let cancels: [(String, String?, (HvfEngineSession, URL) throws -> Void)] = [
            ("pointer press", "clipboard paste canceled: pointer pressed",
             { session, _ in session.sendPointerPress(location: CGPoint(x: 5, y: 5), viewSize: size, imageSize: size) }),
            ("target change", "clipboard paste canceled: input target changed", { session, _ in session.cancelOrderedInputTarget() }),
            ("guest reset", "clipboard paste failed, expired or canceled",
             { session, root in try self.appendLog("PSCI SYSTEM_RESET: reboot 1/8\n", root); session.poll() }),
            ("stop", nil, { session, _ in session.markStopped() }),
            ("attach", nil, { session, _ in XCTAssertTrue(session.attachToRunningVM()) }),
        ]
        for (name, message, cancel) in cancels {
            try withSession { session, root in
                try type(["한"], into: session)
                let first = try lastCommand(session)
                try type(["글", "a", "\u{7f}"], into: session)
                try cancel(session, root)
                XCTAssertNil(session.legacyPaste.pending, name)
                XCTAssertEqual(session.legacyPaste.backlog, [], name)
                if let message {
                    XCTAssertTrue(session.events.contains(.unknown(message + "; discarded 2 held display input entries")), name)
                }
                try acknowledge(first, root: root); session.poll()
                XCTAssertFalse(input(root).contains("KEY"), "\(name): held input must not replay")
                XCTAssertEqual(try commands(session), [first], name)

                if case .stopped = session.connectionState { XCTAssertTrue(session.attachToRunningVM(), name) }
                try appendLog("BVAGENT READY host=synthetic t=3\nBVAGENT SERVICE start t=4\n", root); session.poll()
                try type(["새"], into: session)
                let fresh = try lastCommand(session)
                XCTAssertTrue(fresh.contains(pasted("새")), name)
                try acknowledge(fresh, root: root); session.poll()
                XCTAssertEqual(try commands(session), [first, fresh], "\(name): held text must not start a later paste")
                XCTAssertEqual(input(root).components(separatedBy: "KEY ").count - 1, 1, "\(name): \(input(root))")
                XCTAssertTrue(input(root).hasSuffix("KEY ctrl+v\n"), name)
            }
        }
    }

    func testFullBacklogRefusesVisiblyAndKeepsWhatItHeld() throws {
        try withSession { session, root in
            var beeps = 0
            try type(["한"], into: session)
            let first = try lastCommand(session)
            let limit = HvfLegacyPasteState.maximumBacklog
            try type(Array(repeating: "\u{7f}", count: limit - 1) + ["a", "b"], into: session) { beeps += 1 }
            XCTAssertEqual(beeps, 0, "Text merging into the last entry does not take a new one")
            try type(["\u{7f}", "\r"], into: session) { beeps += 1 }
            XCTAssertEqual(beeps, 2)
            XCTAssertEqual(session.events.last, .unknown("display input refused: 256 entries already wait for a clipboard paste"))
            XCTAssertEqual(input(root), "")

            try acknowledge(first, root: root); session.poll()
            XCTAssertEqual(input(root), "KEY ctrl+v\n" + String(repeating: "KEY backspace\n", count: limit - 1)
                + "KEY text-hex:6162\n")
            try type(["\r"], into: session)
            XCTAssertTrue(input(root).hasSuffix("KEY text-hex:6162\nKEY enter\n"), "Input flows directly again")
        }
    }

    func testMergedDisplayTextSplitsAtTheGuestCommandLimit() throws {
        try withSession { session, root in
            try type(["가"], into: session)
            let first = try lastCommand(session)
            try type(Array(repeating: "한", count: 1_980), into: session)
            try acknowledge(first, root: root); session.poll()
            let second = try lastCommand(session)
            XCTAssertTrue(second.contains(pasted(String(repeating: "한", count: 1_979))))
            XCTAssertEqual(second.utf8.count, HvfClipboardPaste.maximumCommandCharacters - 1)
            try acknowledge(second, root: root); session.poll()
            let third = try lastCommand(session)
            XCTAssertTrue(third.contains(pasted("한")))
            try acknowledge(third, root: root); session.poll()
            XCTAssertEqual(input(root), String(repeating: "KEY ctrl+v\n", count: 3))
            XCTAssertEqual(try commands(session).count, 3)
            XCTAssertEqual(refusals(session), [])
        }
    }

    func testOrderedSessionDisplayInputBypassesTheLegacyBacklog() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session)
            try type(["한", "글", "\u{7f}"], into: session)
            let commands = try fixture.commands()
            XCTAssertEqual(commands.count, 2, "INPUTCAPS, then one TEXTINPUT; the rest queue behind its receipt")
            XCTAssertTrue(commands.last?.hasPrefix("TEXTINPUT ") == true)
            XCTAssertFalse(commands.contains { $0.contains("Set-Clipboard") })
            XCTAssertEqual(refusals(session), [])
            XCTAssertNil(try fixture.bytes(fixture.input))
        }
    }
}
#endif
