import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

@MainActor
final class ControlLibraryCommandsTests: XCTestCase {
    /// File > New VM… (Command-N) brings back a closed library window and opens Create in it.
    func testNewVMShowsTheClosedLibraryWindowWithTheCreateSheet() throws {
        let f = try LibraryDashboardNavigationFixture()
        defer { f.clean() }
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1000, height: 700),
            styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = NSHostingView(rootView: ContentView(library: f.library))
        let previous = ControlLibraryWindow.window
        ControlLibraryWindow.window = window
        defer {
            ControlLibraryWindow.window = previous
            LibraryWindowRequests.shared.createRequested = false
            window.attachedSheet.map { window.endSheet($0) }
            window.close()
            window.contentView = nil
        }
        XCTAssertFalse(window.isVisible)

        ControlLibraryCommands.newVM()

        XCTAssertTrue(window.isVisible, "New VM… shows the library window")
        let deadline = Date().addingTimeInterval(5)
        while window.attachedSheet == nil && Date() < deadline {
            RunLoop.main.run(until: Date().addingTimeInterval(0.05))
        }
        XCTAssertNotNil(window.attachedSheet, "the library window opens its Create sheet")
        XCTAssertFalse(LibraryWindowRequests.shared.createRequested, "the request is used once")
    }

    func testNewVMKeepsAnOpenCreateOrDuplicateSheet() throws {
        let f = try LibraryDashboardNavigationFixture()
        defer { f.clean() }
        let requests = LibraryWindowRequests()
        let iso = URL(fileURLWithPath: "/Volumes/Media/Win11.iso")
        var presentation = LibraryWindowPresentation(sheet: .create(installer: iso))
        requests.createRequested = true
        requests.take(into: &presentation)
        XCTAssertEqual(presentation.sheet, .create(installer: iso))
        XCTAssertFalse(requests.createRequested)

        presentation = LibraryWindowPresentation(sheet: .palette)
        requests.createRequested = true
        requests.take(into: &presentation)
        XCTAssertEqual(presentation.sheet, .create(installer: nil))

        presentation = LibraryWindowPresentation()
        presentation.cloneSource = f.library.vms.first
        requests.createRequested = true
        requests.take(into: &presentation)
        XCTAssertNil(presentation.sheet)
    }
}
