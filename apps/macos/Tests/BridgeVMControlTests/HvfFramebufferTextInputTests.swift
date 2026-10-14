#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfFramebufferTextInputTests: XCTestCase {
    private let automatic = NSRange(location: NSNotFound, length: 0)

    func testFramebufferParticipatesInMacTextInput() {
        let config = HvfEngineConfig(targetDiskPath: "disk", uefiVarsPath: "vars", evidenceDir: "unused",
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false, shareHostDir: nil,
            shareGuestDir: nil, virtioNet: false, virtioGpu3d: false, nvmeBufferedIO: false,
            ctlFilePath: "unused/agent.ctl")
        let session = HvfEngineSession(config: config, repoRoot: URL(fileURLWithPath: "/")) { _ in false }
        let view = FBLayerView(session: session)
        XCTAssertTrue(view is NSTextInputClient, "The live display must let macOS compose text before guest delivery")
        view.teardown()
    }

    func testMarkedHangulStaysLocalUntilOneCommittedResult() {
        let view = RecordingView()
        for text in ["ㅎ", "하", "한"] { mark(text, in: view) }
        XCTAssertEqual(view.markedRange(), NSRange(location: 0, length: 1))
        XCTAssertTrue(view.texts.isEmpty)
        view.insertText(NSAttributedString(string: "한"), replacementRange: automatic)
        view.unmarkText()
        XCTAssertEqual(view.texts, ["한"])
        XCTAssertFalse(view.hasMarkedText())
    }

    func testUnmarkCommitsOnceButCancelNeverCommits() {
        let view = RecordingView()
        mark("한글", in: view)
        view.unmarkText(); view.unmarkText()
        XCTAssertEqual(view.texts, ["한글"])
        mark("discard", in: view)
        view.cancelTextComposition(); view.unmarkText()
        XCTAssertFalse(view.hasMarkedText())
        XCTAssertEqual(view.texts, ["한글"])
    }

    func testCompositionUsesUTF16RangesAndRejectsGuestDocumentReplacement() {
        let view = RecordingView()
        mark("😀한", in: view)
        XCTAssertEqual(view.markedRange(), NSRange(location: 0, length: 3))
        XCTAssertEqual(view.selectedRange(), NSRange(location: 3, length: 0))
        view.setMarkedText("글", selectedRange: NSRange(location: 1, length: 0),
            replacementRange: NSRange(location: 2, length: 1))
        var actual = automatic
        XCTAssertEqual(view.attributedSubstring(forProposedRange: NSRange(location: 0, length: 99),
            actualRange: &actual)?.string, "😀글")
        XCTAssertEqual(actual, NSRange(location: 0, length: 3))
        XCTAssertNil(view.attributedSubstring(forProposedRange: NSRange(location: 1, length: 1), actualRange: nil))
        XCTAssertNil(view.attributedSubstring(forProposedRange: NSRange(location: Int.min, length: 1), actualRange: nil))
        XCTAssertNil(view.attributedSubstring(forProposedRange: NSRange(location: 0, length: Int.min), actualRange: nil))
        view.insertText("BAD", replacementRange: NSRange(location: 9, length: 1))
        XCTAssertEqual(view.refusals, 1)
        XCTAssertTrue(view.texts.isEmpty)
        view.insertText("한", replacementRange: NSRange(location: 2, length: 1))
        XCTAssertEqual(view.texts, ["😀한"])
        view.insertText("BAD", replacementRange: NSRange(location: 0, length: 1))
        XCTAssertEqual(view.refusals, 2, "Committed guest text is outside this client's storage")
    }

    func testInvalidMarkedSelectionPreservesExistingComposition() {
        let view = RecordingView()
        mark("한", in: view)
        view.setMarkedText("😀", selectedRange: NSRange(location: 1, length: 0), replacementRange: automatic)
        XCTAssertEqual(view.refusals, 1)
        view.unmarkText()
        XCTAssertEqual(view.texts, ["한"])
    }

    func testRefusedCommitIsNotReplayedByUnmark() {
        let view = RecordingView()
        view.acceptText = false
        mark("한", in: view)
        view.insertText("한", replacementRange: automatic)
        view.unmarkText()
        XCTAssertEqual(view.texts, ["한"])
        XCTAssertEqual(view.refusals, 1)
    }

    func testPhysicalKeysBypassInterpreterAndCompositionNavigationDoesNot() throws {
        let view = RecordingView()
        view.interceptInterpretation = true
        view.keyDown(with: try event("\r"))
        XCTAssertEqual(view.keys, ["\r"])
        XCTAssertEqual(view.interpreted.count, 0)
        mark("한", in: view)
        view.keyDown(with: try event("\u{f702}"))
        XCTAssertEqual(view.interpreted, ["\u{f702}"])
        XCTAssertEqual(view.keys, ["\r"])
        view.keyDown(with: try event("\u{3}", flags: .control))
        XCTAssertFalse(view.hasMarkedText())
        XCTAssertEqual(view.keys, ["\r", "\u{3}"])
        XCTAssertTrue(view.texts.isEmpty)
    }

    func testOptionTextGoesThroughInterpreterAndAltTabRemainsPhysical() throws {
        let view = RecordingView()
        view.interceptInterpretation = true
        view.keyDown(with: try event("´", flags: .option))
        view.keyDown(with: try event("\t", flags: .option))
        XCTAssertEqual(view.interpreted, ["´"])
        XCTAssertEqual(view.keys, ["\t"])
    }

    func testAppKitInterpreterDeliversASCIIThroughInsertText() throws {
        let view = RecordingView()
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 320, height: 200),
            styleMask: [], backing: .buffered, defer: true)
        window.contentView = view
        defer { window.contentView = nil }
        XCTAssertTrue(window.makeFirstResponder(view))
        view.keyDown(with: try event("a", keyCode: 0))
        XCTAssertEqual(view.texts, ["a"])
        XCTAssertTrue(view.keys.isEmpty)
    }

    func testResigningResponderDiscardsMarkedText() {
        let view = RecordingView()
        mark("한", in: view)
        XCTAssertTrue(view.resignFirstResponder())
        view.unmarkText()
        XCTAssertTrue(view.texts.isEmpty)
        XCTAssertFalse(view.hasMarkedText())
    }

    func testReentrantTargetChangeRejectsRemainingCallbacksFromSameEvent() throws {
        let view = RecordingView()
        view.duringInterpretation = { input in
            input.insertText("한", replacementRange: self.automatic)
            input.targetGeneration = UUID()
            input.doCommand(by: #selector(NSResponder.insertNewline(_:)))
            input.setMarkedText("글", selectedRange: NSRange(location: 1, length: 0), replacementRange: self.automatic)
            input.insertText("글", replacementRange: self.automatic)
        }
        view.keyDown(with: try event("a"))
        XCTAssertEqual(view.texts, ["한"])
        XCTAssertTrue(view.keys.isEmpty)
        XCTAssertFalse(view.hasMarkedText())
    }

    func testRefusedCommitStopsFollowingCommandFromSameEvent() throws {
        let view = RecordingView()
        view.acceptText = false
        view.duringInterpretation = { input in
            input.insertText("한", replacementRange: self.automatic)
            input.doCommand(by: #selector(NSResponder.insertNewline(_:)))
        }
        view.keyDown(with: try event("a"))
        XCTAssertEqual(view.texts, ["한"])
        XCTAssertTrue(view.keys.isEmpty)
        XCTAssertEqual(view.refusals, 1)
    }

    private func mark(_ text: String, in view: RecordingView) {
        view.setMarkedText(text, selectedRange: NSRange(location: text.utf16.count, length: 0), replacementRange: automatic)
    }

    private func event(_ characters: String, flags: NSEvent.ModifierFlags = [], keyCode: UInt16 = 0) throws -> NSEvent {
        try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: flags,
            timestamp: 0, windowNumber: 0, context: nil, characters: characters,
            charactersIgnoringModifiers: characters, isARepeat: false, keyCode: keyCode))
    }

    private final class RecordingView: HvfFramebufferTextInput {
        var texts: [String] = [], keys: [String] = [], interpreted: [String] = []
        var refusals = 0, acceptText = true, interceptInterpretation = false
        var targetGeneration = UUID()
        var duringInterpretation: ((RecordingView) -> Void)?
        override var acceptsFirstResponder: Bool { true }
        override var guestInputAvailable: Bool { true }
        override var guestInputIdentity: HvfTextInputTargetIdentity? {
            .init(owner: ObjectIdentifier(self), generation: targetGeneration, binding: [])
        }
        override func sendGuestText(_ text: String) -> Bool { texts.append(text); return acceptText }
        override func sendGuestKeyEvent(_ event: NSEvent) -> Bool { keys.append(event.characters ?? ""); return true }
        override func textInputRefused() { refusals += 1 }
        override func interpretKeyEvents(_ events: [NSEvent]) {
            if let duringInterpretation { duringInterpretation(self) }
            else if interceptInterpretation { interpreted += events.compactMap(\.characters) }
            else { super.interpretKeyEvents(events) }
        }
    }
}
#endif
