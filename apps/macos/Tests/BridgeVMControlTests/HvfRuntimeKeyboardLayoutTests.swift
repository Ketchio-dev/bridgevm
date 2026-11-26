import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeKeyboardLayoutTests: XCTestCase {
    func testCompactDraftFieldShowsItsWholePrompt() async throws {
        let fixture = try HvfOwnedRuntimeFixture(readyForLaunch: false)
        defer { fixture.clean() }
        let host = NSHostingView(rootView: HvfRuntimeKeyboardInput(session: fixture.session(), draft: .constant("")))
        host.sizingOptions = []
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 324, height: 80),
                              styleMask: [.borderless], backing: .buffered, defer: true)
        window.contentView = host
        defer { window.contentView = nil }
        await Task.yield()
        host.layoutSubtreeIfNeeded()

        func editableField(in view: NSView) -> NSTextField? {
            if let field = view as? NSTextField, field.isEditable { return field }
            return view.subviews.lazy.compactMap { editableField(in: $0) }.first
        }
        let field = try XCTUnwrap(editableField(in: host))
        let prompt = try XCTUnwrap(field.placeholderString)
        XCTAssertEqual(prompt, "Text to send to Windows")
        let font = try XCTUnwrap(field.font)
        let promptWidth = (prompt as NSString).size(withAttributes: [.font: font]).width
        XCTAssertGreaterThanOrEqual(field.bounds.width, promptWidth)
    }
}
