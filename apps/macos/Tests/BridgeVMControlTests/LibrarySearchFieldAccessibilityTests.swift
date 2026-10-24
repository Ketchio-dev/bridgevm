import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

/// The library search field's AppKit accessibility element carries its identifier, and an
/// accessibility value set on it, which is what the accessibility server calls for a client's
/// AXUIElementSetAttributeValue, reaches the query the list filters by. d6 run 4 (2026-10-23,
/// seal 8d3eb052) set "Indigo" and read it back, but the list never filtered. An AppKit text
/// field is accessible through its cell, so the cell is the element checked here. The window
/// is deferred and never shown.
@MainActor
final class LibrarySearchFieldAccessibilityTests: XCTestCase {
    private final class Model: ObservableObject { @Published var query = "" }
    private struct Host: View {
        @ObservedObject var model: Model
        var body: some View { LibrarySearchField(query: $model.query) }
    }

    func testAnAccessibilityValueSetOnTheFieldReachesTheQuery() async throws {
        let model = Model()
        let host = NSHostingView(rootView: Host(model: model))
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 480, height: 80),
                              styleMask: [], backing: .buffered, defer: true)
        window.contentView = host
        defer { window.contentView = nil }
        await settle(host)
        let fields = editable(host)
        XCTAssertEqual(fields.count, 1, "one editable text field")
        let field = try XCTUnwrap(fields.first)
        let element = try XCTUnwrap(field.cell)
        XCTAssertTrue(element.isAccessibilityElement())
        XCTAssertEqual(element.accessibilityRole(), .textField)
        XCTAssertEqual(element.accessibilityIdentifier(), "bridgevm.library.search", "the element carries the identifier")
        XCTAssertFalse(field.isAccessibilityElement() || field.accessibilityIdentifier() == "bridgevm.library.search",
                       "only the element carries it, so an accessibility walk finds the field once")
        XCTAssertEqual(field.placeholderString, "VM 이름 또는 엔진 검색")
        element.setAccessibilityValue("Indigo")
        await settle(host)
        XCTAssertEqual(element.accessibilityValue() as? String, "Indigo", "the element reads back the value")
        XCTAssertEqual(model.query, "Indigo", "the value set reaches the query")
        model.query = ""
        await settle(host)
        XCTAssertEqual(field.stringValue, "", "emptying the query, as the clear button does, empties the field")
        field.setAccessibilityValue("Amber")
        await settle(host)
        XCTAssertEqual(model.query, "Amber", "a set on the control reaches the query too")
        field.stringValue = "Typed"
        field.delegate?.controlTextDidChange?(Notification(name: NSControl.textDidChangeNotification, object: field))
        await settle(host)
        XCTAssertEqual(model.query, "Typed", "typing still reaches the query")
    }

    private func editable(_ view: NSView) -> [NSTextField] {
        ((view as? NSTextField).map { $0.isEditable ? [$0] : [] } ?? []) + view.subviews.flatMap(editable)
    }

    private func settle(_ host: NSView) async {
        for _ in 0..<5 { await Task.yield() }
        host.layoutSubtreeIfNeeded()
    }
}
