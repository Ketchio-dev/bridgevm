#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfTextCompositionViewportTests: XCTestCase {
    func testLongPreeditAndCandidateAnchorStayInsideResizedGuestDisplay() throws {
        let view = CompositionView(frame: NSRect(x: 0, y: 0, width: 400, height: 240))
        let window = NSWindow(contentRect: view.frame, styleMask: [.resizable], backing: .buffered, defer: true)
        window.contentView = view
        defer { window.contentView = nil }
        let text = String(repeating: "한", count: 40)
        let caret = NSRange(location: text.utf16.count, length: 0)
        view.setMarkedText(text, selectedRange: caret, replacementRange: NSRange(location: NSNotFound, length: 0))
        for width: CGFloat in [400, 240, 500] {
            window.setContentSize(NSSize(width: width, height: 240))
            let candidate = view.firstRect(forCharacterRange: caret, actualRange: nil)
            let content = window.convertToScreen(window.contentLayoutRect)
            let overlay = try XCTUnwrap(view.subviews.first as? NSTextField)
            XCTAssertEqual(view.bounds.width, width, "Preedit must not expand the guest display beyond its window")
            XCTAssertLessThanOrEqual(overlay.frame.maxX, width - 12)
            XCTAssertLessThanOrEqual(candidate.maxX, content.maxX, "IME candidates must anchor inside the visible window")
            XCTAssertEqual(overlay.stringValue, text, "Clipping must preserve the full composition for commit")
            XCTAssertEqual(view.markedRange().length, text.utf16.count)
        }
        view.unmarkText()
        XCTAssertEqual(view.commits, [text])
    }

    private final class CompositionView: HvfFramebufferTextInput {
        var commits: [String] = []
        override var guestInputAvailable: Bool { true }
        override var isFlipped: Bool { true }
        override func sendGuestText(_ text: String) -> Bool { commits.append(text); return true }
    }
}
#endif
