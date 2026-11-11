#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

/// Composing text through the real display view and session: an agent that advertised CARET is asked once per
/// composition, and its answer puts the composing-text overlay and the IME's anchor (firstRect) just below the
/// guest's caret. Without the marker, or without an answer, both stay at the bottom-left as before.
@MainActor
final class HvfGuestCaretCompositionTests: XCTestCase {
    private let automatic = NSRange(location: NSNotFound, length: 0)

    /// Before this change the candidate window always anchored 12 pt from the display's bottom-left.
    func testTheCandidateAnchorSitsJustBelowTheGuestCaret() throws {
        try withComposingDisplay(caret: true) { view, session, fixture in
            mark("ㅎ", in: view)
            let question = try XCTUnwrap(try fixture.commands().last)
            XCTAssertTrue(question.hasPrefix("CARET "), question)
            XCTAssertNotNil(UUID(uuidString: String(question.dropFirst("CARET ".count))))
            // A 64x40 guest in a 320x200 view: 5 points per guest pixel, no bars.
            try fixture.appendLog("BVAGENT CMD \(question) exit=0\nBVCARET 10 8 1 4\nBVAGENT END \(question)\n")
            session.poll()
            let anchor = try anchorRect(view)
            XCTAssertEqual(anchor.minX, 50, accuracy: 0.5, "at the caret's x")
            XCTAssertEqual(anchor.minY, 62, accuracy: 0.5, "just below the caret, which ends at y=60")
            let overlay = try XCTUnwrap(view.subviews.first as? NSTextField)
            XCTAssertEqual(overlay.frame.minX, 50, accuracy: 0.5)
            XCTAssertEqual(overlay.frame.minY, 62, accuracy: 0.5)
            XCTAssertNil(session.guestCaretQuery)

            // Composing on keeps the caret without asking again; committing ends it, and the next composition asks,
            // but only once the committed text has gone in: it moves the caret.
            mark("하", in: view)
            XCTAssertEqual(try anchorRect(view).minY, 62, accuracy: 0.5)
            view.unmarkText()
            mark("ㄱ", in: view)
            session.poll()
            XCTAssertEqual(try fixture.commands().filter { $0.hasPrefix("CARET ") }.count, 1,
                           "asked while the committed syllable was still on its way: the old caret")
            try acknowledgeText(fixture, session)
            XCTAssertEqual(try fixture.commands().filter { $0.hasPrefix("CARET ") }.count, 2)
            XCTAssertEqual(try fixture.commands().last?.hasPrefix("CARET "), true)
            XCTAssertGreaterThan(try anchorRect(view).minY, 150, "until it is answered: the bottom-left")
        }
    }

    func testWithoutTheMarkerTheGuestIsNeverAskedAndTheAnchorStaysBottomLeft() throws {
        try withComposingDisplay(caret: false) { view, _, fixture in
            mark("ㅎ", in: view)
            XCTAssertFalse(try fixture.commands().contains { $0.hasPrefix("CARET") })
            let anchor = try anchorRect(view)
            XCTAssertEqual(anchor.minX, 12, accuracy: 0.5)
            XCTAssertGreaterThan(anchor.minY, 150)
        }
    }

    func testNoneAStaleAnswerOrNoAnswerLeavesTheBottomLeft() throws {
        try withComposingDisplay(caret: true) { view, session, fixture in
            // `none`.
            mark("ㅎ", in: view)
            var question = try XCTUnwrap(try fixture.commands().last)
            try fixture.appendLog("BVAGENT CMD \(question) exit=0\nBVCARET none\nBVAGENT END \(question)\n")
            session.poll()
            XCTAssertGreaterThan(try anchorRect(view).minY, 150)
            view.cancelTextComposition()

            // Asked again while a question is on the wire: one question at a time, its answer (which may predate
            // input sent since) is dropped, and the question goes once more for the latest composition.
            mark("ㅎ", in: view)
            question = try XCTUnwrap(try fixture.commands().last)
            view.cancelTextComposition()
            mark("ㅎ", in: view)
            XCTAssertEqual(try fixture.commands().last, question)
            try fixture.appendLog("BVAGENT CMD \(question) exit=0\nBVCARET 10 8 1 4\nBVAGENT END \(question)\n")
            session.poll()
            XCTAssertGreaterThan(try anchorRect(view).minY, 150)
            let again = try XCTUnwrap(try fixture.commands().last)
            XCTAssertTrue(again.hasPrefix("CARET ") && again != question, again)
            try fixture.appendLog("BVAGENT CMD \(again) exit=0\nBVCARET 20 8 1 4\nBVAGENT END \(again)\n")
            session.poll()
            XCTAssertEqual(try anchorRect(view).minX, 100, accuracy: 0.5, "the fresh answer counts")
            XCTAssertNil(session.guestCaretQuery)
            view.cancelTextComposition()

            // No answer: the question ends at its deadline, and the next composition asks again.
            mark("ㅎ", in: view)
            question = try XCTUnwrap(try fixture.commands().last)
            let asked = session.hostNow()
            session.hostNow = { asked.addingTimeInterval(HvfGuestCaretRequest.timeout) }
            session.poll()
            XCTAssertNil(session.guestCaretQuery)
            XCTAssertGreaterThan(try anchorRect(view).minY, 150)
            view.cancelTextComposition()
            mark("ㅎ", in: view)
            XCTAssertNotEqual(try fixture.commands().last, question)
            XCTAssertEqual(try fixture.commands().last?.hasPrefix("CARET "), true)
        }
    }

    /// The ordered TEXTINPUT on the wire acknowledged as inserted, as the agent would.
    private func acknowledgeText(_ fixture: HvfRuntimeKeyboardDraftFixture, _ session: HvfEngineSession) throws {
        session.poll()
        let line = try XCTUnwrap(try fixture.commands().last { $0.hasPrefix("TEXTINPUT ") })
        let words = line.split(separator: " ").map(String.init)
        let data = try XCTUnwrap(Data(base64Encoded: words[2]))
        let count = try XCTUnwrap(HvfGuestInputEncoding(.text(String(decoding: data, as: UTF8.self)))?.insertedEventCount)
        try fixture.appendLog("BVAGENT CMD TEXTINPUT \(words[1]) exit=0\nBVINPUT_INSERTED \(words[1]) \(count)\nBVAGENT END TEXTINPUT \(words[1])\n")
        session.poll()
    }

    private func anchorRect(_ view: NSView & NSTextInputClient) throws -> NSRect {
        let screen = view.firstRect(forCharacterRange: NSRange(location: 0, length: 0), actualRange: nil)
        let window = try XCTUnwrap(view.window)
        return view.convert(window.convertFromScreen(screen), from: nil)
    }

    private func mark(_ text: String, in view: FBLayerView) {
        view.setMarkedText(text, selectedRange: NSRange(location: text.utf16.count, length: 0), replacementRange: automatic)
    }

    /// A 64x40 guest display in a 320x200 window, ordered input negotiated with or without the CARET marker.
    private func withComposingDisplay(caret: Bool,
                                      _ body: (FBLayerView, HvfEngineSession, HvfRuntimeKeyboardDraftFixture) throws -> Void) throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        let (width, height) = (64, 40)
        var frame = Data(count: 64 + width * 4 * height)
        for (offset, value): (Int, UInt32) in [(0, 0x42564642), (8, UInt32(width)), (12, UInt32(height)), (16, UInt32(width * 4)), (24, 2)] {
            var little = value.littleEndian
            withUnsafeBytes(of: &little) { frame.replaceSubrange(offset..<(offset + 4), with: $0) }
        }
        try frame.write(to: fixture.evidence.appendingPathComponent("display.fb"))
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session, modifierChords: true, text: caret ? ["CARET"] : [])
            XCTAssertEqual(session.guestCaret, caret)
            let view = FBLayerView(session: session)
            defer { view.teardown() }
            view.frame = NSRect(x: 0, y: 0, width: 320, height: 200)
            let window = NSWindow(contentRect: view.frame, styleMask: [], backing: .buffered, defer: true)
            window.contentView = view
            defer { window.contentView = nil }
            XCTAssertTrue(window.makeFirstResponder(view))
            view.refreshFrame(at: 0)
            XCTAssertTrue(view.guestInputAvailable)
            try body(view, session, fixture)
        }
    }
}

/// The composition's caret over its lifetime, with the guest's answer given directly.
@MainActor
final class HvfGuestCaretPlacementTests: XCTestCase {
    private let automatic = NSRange(location: NSNotFound, length: 0)

    /// Korean commits a syllable and marks the next in one key event: the overlay stays at the caret instead of
    /// jumping to the bottom-left per syllable, and the question in flight is not asked twice.
    func testASyllableChainKeepsTheCaretAndAKeyThatEndsTheCompositionDropsIt() throws {
        let (view, window) = try composingView()
        defer { window.contentView = nil }
        view.duringInterpretation = { $0.setMarkedText("ㅎ", selectedRange: NSRange(location: 1, length: 0), replacementRange: self.automatic) }
        view.keyDown(with: try event("g"))
        XCTAssertEqual(view.questions.count, 1)
        view.questions[0](NSRect(x: 100, y: 40, width: 2, height: 18))
        XCTAssertEqual(try overlay(view).frame.minY, 60, accuracy: 0.5)
        view.duringInterpretation = { input in
            input.insertText("한", replacementRange: self.automatic)
            input.setMarkedText("ㄱ", selectedRange: NSRange(location: 1, length: 0), replacementRange: self.automatic)
        }
        view.keyDown(with: try event("r"))
        XCTAssertEqual(view.commits, ["한"])
        XCTAssertEqual(try overlay(view).frame.minX, 100, accuracy: 0.5)
        XCTAssertEqual(try overlay(view).frame.minY, 60, accuracy: 0.5)
        // A late answer to the second question still counts while composing goes on.
        XCTAssertEqual(view.questions.count, 2)
        view.questions[1](NSRect(x: 130, y: 40, width: 2, height: 18))
        XCTAssertEqual(try overlay(view).frame.minX, 130, accuracy: 0.5)

        view.duringInterpretation = { $0.insertText("글", replacementRange: self.automatic) }
        view.keyDown(with: try event(" "))
        XCTAssertEqual(view.commits, ["한", "글"])
        view.duringInterpretation = { $0.setMarkedText("ㅎ", selectedRange: NSRange(location: 1, length: 0), replacementRange: self.automatic) }
        view.keyDown(with: try event("g"))
        XCTAssertEqual(try overlay(view).frame.minX, 12, accuracy: 0.5, "a new composition starts at the bottom-left")
        view.questions[0](NSRect(x: 100, y: 40, width: 2, height: 18))
        XCTAssertEqual(try overlay(view).frame.minX, 12, accuracy: 0.5, "an answer from before the commit is stale")
    }

    func testTheOverlayStaysInsideAResizedViewAndAResizeDropsTheCaret() throws {
        let (view, window) = try composingView()
        defer { window.contentView = nil }
        view.setMarkedText("한글", selectedRange: NSRange(location: 2, length: 0), replacementRange: automatic)
        view.questions.last?(NSRect(x: 395, y: 230, width: 2, height: 18))
        let placed = try overlay(view).frame
        XCTAssertGreaterThanOrEqual(388.5, placed.maxX, "inside the right margin")
        XCTAssertGreaterThanOrEqual(228.5, placed.maxY, "above the caret when there is no room below")
        window.setContentSize(NSSize(width: 300, height: 240))
        XCTAssertEqual(try overlay(view).frame.minX, 12, accuracy: 0.5)
        XCTAssertGreaterThan(try overlay(view).frame.minY, 190)
    }

    private func overlay(_ view: NSView) throws -> NSTextField {
        view.layoutSubtreeIfNeeded()
        return try XCTUnwrap(view.subviews.first as? NSTextField)
    }

    private func composingView() throws -> (CaretView, NSWindow) {
        let view = CaretView(frame: NSRect(x: 0, y: 0, width: 400, height: 240))
        let window = NSWindow(contentRect: view.frame, styleMask: [.resizable], backing: .buffered, defer: true)
        window.contentView = view
        XCTAssertTrue(window.makeFirstResponder(view))
        return (view, window)
    }

    private func event(_ characters: String) throws -> NSEvent {
        try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: 0,
            context: nil, characters: characters, charactersIgnoringModifiers: characters, isARepeat: false, keyCode: 5))
    }

    private final class CaretView: HvfFramebufferTextInput {
        var commits: [String] = []
        /// Each question's answer, in order; a question is asked only while none is open, as the session does.
        var questions: [(NSRect?) -> Void] = []
        private var open = false
        var duringInterpretation: ((CaretView) -> Void)?
        override var acceptsFirstResponder: Bool { true }
        override var guestInputAvailable: Bool { true }
        override var isFlipped: Bool { true }
        override func sendGuestText(_ text: String) -> Bool { commits.append(text); return true }
        override func sendGuestKeyEvent(_ event: NSEvent) -> Bool { true }
        override func askGuestCaret(_ found: @escaping (NSRect?) -> Void) -> Bool {
            guard !open else { return false }
            open = true
            questions.append { [weak self] caret in self?.open = false; found(caret) }
            return true
        }
        override func interpretKeyEvents(_ events: [NSEvent]) { duringInterpretation?(self) }
    }
}
#endif
