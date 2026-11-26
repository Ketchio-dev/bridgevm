import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryDashboardNavigationTests: XCTestCase {
    func testPaletteProModeReachesFleetAndReturnsToSameSelectedDashboard() throws {
        let f = try LibraryDashboardNavigationFixture()
        defer { f.clean() }
        let model = try XCTUnwrap(f.library.selectedModel)
        XCTAssertEqual(f.values(LibraryDashboardDetail.self, in: LibraryDashboardRoute(library: f.library).body).count, 1)
        try f.paletteCommand("Pro 모드 켜기").action()
        XCTAssertTrue(f.library.proMode)
        let route = try f.detailRoute()
        XCTAssertEqual(f.values(FleetTableView.self, in: route.body).count, 1)
        XCTAssertEqual(f.library.selectedID, model.config.slug)
        XCTAssertTrue(f.library.selectedModel === model)
        try f.paletteCommand("Pro 모드 끄기").action()
        XCTAssertFalse(f.library.proMode)
        let restored = try XCTUnwrap(f.values(LibraryDashboardDetail.self,
            in: LibraryDashboardRoute(library: f.library).body).first)
        XCTAssertTrue(restored.model === model)
        XCTAssertEqual(restored.config, model.config)
        XCTAssertFalse(model.running)
        XCTAssertFalse(model.lifecycleBusy)
    }

    func testSearchFieldReturnOpensTheOnlyMatchingVM() async throws {
        let f = try LibraryDashboardNavigationFixture()
        defer { f.clean() }
        var duplicate = try XCTUnwrap(f.library.vms.first)
        duplicate.name = "서울"
        duplicate.id = "seoul-a"
        f.library.vms.append(duplicate)
        duplicate.id = "seoul-b"
        f.library.vms.append(duplicate)
        f.library.selectedID = nil
        let host = NSHostingView(rootView: CommandPaletteView(library: f.library) { XCTFail("Unexpected Create action") })
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 480, height: 400),
            styleMask: [.titled], backing: .buffered, defer: true)
        window.contentView = host
        defer { window.contentView = nil }
        await Task.yield()
        host.layoutSubtreeIfNeeded()

        func editableField(in view: NSView) -> NSTextField? {
            if let field = view as? NSTextField, field.isEditable { return field }
            return view.subviews.lazy.compactMap { editableField(in: $0) }.first
        }
        let field = try XCTUnwrap(editableField(in: host))
        XCTAssertTrue(window.makeFirstResponder(field))
        let enter = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [],
            timestamp: 0, windowNumber: window.windowNumber, context: nil, characters: "\r",
            charactersIgnoringModifiers: "\r", isARepeat: false, keyCode: 36))

        // A blank search lists several actions, including Create.
        window.sendEvent(enter)
        XCTAssertNil(f.library.selectedID)
        XCTAssertTrue(window.makeFirstResponder(field))
        let searchEditor = try XCTUnwrap(field.currentEditor() as? NSTextView)
        searchEditor.insertText("Owned", replacementRange: NSRange(location: 0, length: 0))
        await Task.yield()
        host.layoutSubtreeIfNeeded()
        searchEditor.setMarkedText("Owned", selectedRange: NSRange(location: 5, length: 0),
                                   replacementRange: NSRange(location: 0, length: 5))
        XCTAssertTrue(searchEditor.hasMarkedText())
        window.sendEvent(enter)
        XCTAssertNil(f.library.selectedID, "Return while composing belongs to the input method")

        searchEditor.insertText("서울", replacementRange: NSRange(location: 0, length: 5))
        await Task.yield()
        window.sendEvent(enter)
        XCTAssertNil(f.library.selectedID)
        let down = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [.numericPad, .function],
            timestamp: 0, windowNumber: window.windowNumber, context: nil, characters: "\u{f701}",
            charactersIgnoringModifiers: "\u{f701}", isARepeat: false, keyCode: 125))
        window.sendEvent(down)
        window.sendEvent(down)
        window.sendEvent(enter)
        XCTAssertEqual(f.library.selectedID, "seoul-b")

        f.library.selectedID = nil
        searchEditor.insertText("Owned", replacementRange: NSRange(location: 0, length: 2))
        await Task.yield()
        window.sendEvent(enter)
        XCTAssertEqual(f.library.selectedID, "owned")
    }

    func testExplicitImportSelectionKeepsFirstRunAheadOfFleet() throws {
        let f = try LibraryDashboardNavigationFixture()
        defer { f.clean() }
        f.library.selectedID = LibraryModel.firstRunImportSelectionID
        let operation = f.library.firstRunImport.begin()
        f.library.firstRunImport.advance(.copying, operation: operation)
        try f.paletteCommand("Pro 모드 켜기").action()
        let route = try f.detailRoute()
        XCTAssertEqual(f.values(FirstRunView.self, in: route.body).count, 1)
        XCTAssertTrue(f.values(FleetTableView.self, in: route.body).isEmpty)
        XCTAssertTrue(f.library.firstRunImportBusy)
        XCTAssertEqual(f.library.firstRunImport.stage, .copying)
        XCTAssertEqual(f.library.selectedID, LibraryModel.firstRunImportSelectionID)
    }

    func testRetainedControlSelectionKeepsExactHandleAheadOfFleet() throws {
        let f = try LibraryDashboardNavigationFixture()
        defer { f.clean() }
        let model = try XCTUnwrap(f.library.selectedModel)
        model.running = true // An observed active control; no process is launched or adopted.
        f.library.reload() // The synthetic registration is absent, so its active handle is retained.
        let retained = try XCTUnwrap(f.library.selectedRetainedControl)
        try f.paletteCommand("Pro 모드 켜기").action()
        let route = try f.detailRoute()
        let view = try XCTUnwrap(f.values(RetainedControlDetailView.self, in: route.body).first)
        XCTAssertEqual(view.record.id, retained.id)
        XCTAssertTrue(f.values(FleetTableView.self, in: route.body).isEmpty)
        XCTAssertEqual(f.library.selectedID, retained.id)
        XCTAssertFalse(f.library.dismissRetainedControl(retained.id))
        guard case let .generic(_, actual) = view.record.descriptor else { return XCTFail("Expected retained generic control") }
        XCTAssertTrue(actual === model)
        XCTAssertTrue(model.running)
    }
}
