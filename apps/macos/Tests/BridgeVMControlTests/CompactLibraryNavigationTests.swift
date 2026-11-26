import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

@MainActor
final class CompactLibraryNavigationTests: XCTestCase {
    func testNarrowWindowCanSearchAndReturnToItsSelectedDashboard() async throws {
        let fixture = try LibraryDashboardNavigationFixture()
        defer { fixture.clean() }
        let host = NSHostingController(rootView: ContentView(library: fixture.library))
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 640, height: 720),
                              styleMask: [.titled, .resizable], backing: .buffered, defer: true)
        window.contentViewController = host
        window.setContentSize(NSSize(width: 640, height: 720))
        defer { window.contentViewController = nil }

        func search(in view: NSView) -> NSTextField? {
            if let field = view as? NSTextField, field.cell?.accessibilityIdentifier() == "bridgevm.library.search" {
                return field
            }
            return view.subviews.lazy.compactMap { search(in: $0) }.first
        }
        func waitFor(_ ready: () -> Bool) async -> Bool {
            for _ in 0..<50 {
                host.view.layoutSubtreeIfNeeded()
                if ready() { return true }
                try? await Task.sleep(for: .milliseconds(10))
            }
            return false
        }
        func command(_ text: String, code: UInt16) throws {
            let event = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: .command,
                timestamp: 0, windowNumber: window.windowNumber, context: nil, characters: text,
                charactersIgnoringModifiers: text, isARepeat: false, keyCode: code))
            XCTAssertTrue(window.performKeyEquivalent(with: event))
        }

        host.view.layoutSubtreeIfNeeded()
        XCTAssertEqual(host.view.bounds.width, 640, accuracy: 1)
        XCTAssertNil(search(in: host.view), "The selected VM should have the full compact window")
        try command("f", code: 3)
        let focused = await waitFor {
            guard let editor = search(in: host.view)?.currentEditor() else { return false }
            return window.firstResponder === editor
        }
        XCTAssertTrue(focused)
        let field = try XCTUnwrap(search(in: host.view))
        let cell = try XCTUnwrap(field.cell)
        cell.setAccessibilityValue("No matching VM")
        XCTAssertEqual((field.currentEditor() as? NSTextView)?.string, "No matching VM")
        try fixture.paletteCommand("Turn on Pro mode").action()
        let comparing = await waitFor { search(in: host.view) == nil }
        XCTAssertTrue(comparing)
        try fixture.paletteCommand("Turn off Pro mode").action()
        let listRestored = await waitFor { search(in: host.view)?.stringValue == "No matching VM" }
        XCTAssertTrue(listRestored, "Leaving comparison returns to the same filtered library")
        try command("]", code: 30)
        let returned = await waitFor { search(in: host.view) == nil }
        XCTAssertTrue(returned, "Forward returns to the chosen VM even with an empty filtered list")
        XCTAssertEqual(fixture.library.selectedID, "owned")
        XCTAssertFalse(window.isVisible)
    }

    func testResizeKeepsUnsubmittedImportFields() async throws {
        let fixture = try LibraryDashboardNavigationFixture()
        defer { fixture.clean() }
        fixture.library.selectedID = LibraryModel.firstRunImportSelectionID
        let host = NSHostingController(rootView: ContentView(library: fixture.library))
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 640, height: 720),
                              styleMask: [.titled, .resizable], backing: .buffered, defer: true)
        window.contentViewController = host
        window.setContentSize(NSSize(width: 640, height: 720))
        defer { window.contentViewController = nil }
        func nameField(_ view: NSView) -> NSTextField? {
            if let field = view as? NSTextField, field.isEditable, field.placeholderString == "VM name" {
                return field
            }
            return view.subviews.lazy.compactMap(nameField).first
        }
        host.view.layoutSubtreeIfNeeded()
        let field = try XCTUnwrap(nameField(host.view))
        XCTAssertTrue(window.makeFirstResponder(field))
        let editor = try XCTUnwrap(field.currentEditor() as? NSTextView)
        editor.selectAll(nil)
        editor.insertText("Unsubmitted import", replacementRange: NSRange(location: NSNotFound, length: 0))
        window.makeFirstResponder(nil)
        for width in [1024, 640] {
            window.setContentSize(NSSize(width: width, height: 720))
            await Task.yield()
            host.view.layoutSubtreeIfNeeded()
            XCTAssertEqual(host.view.bounds.width, CGFloat(width), accuracy: 1)
            XCTAssertEqual(nameField(host.view)?.stringValue, "Unsubmitted import")
        }
        XCTAssertFalse(window.isVisible)
    }
}
