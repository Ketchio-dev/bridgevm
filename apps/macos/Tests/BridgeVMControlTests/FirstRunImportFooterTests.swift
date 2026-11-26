import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

@MainActor
final class FirstRunImportFooterTests: XCTestCase {
    func testImportButtonStaysVisibleWhileSmallWindowScrollsThroughOptionalSettings() async throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let library = LibraryModel(rootURL: fixture.library, migrateLegacy: false, startsModelsAutomatically: false)
        library.selectedID = LibraryModel.firstRunImportSelectionID
        let host = NSHostingView(rootView: FirstRunView(library: library).groupBoxStyle(LibraryGroupBoxStyle()))
        host.sizingOptions = []
        let window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 640, height: 460),
                              styleMask: [], backing: .buffered, defer: true)
        window.contentView = host
        defer { window.contentView = nil }
        host.frame = window.contentLayoutRect
        await FirstRunOptionalRecoveryFixture.settle(host)
        let fields = FirstRunOptionalRecoveryFixture.fields(host)
        XCTAssertEqual(fields.count, 4)
        let fileField = try XCTUnwrap(fields.dropFirst().first)
        FirstRunOptionalRecoveryFixture.edit(fileField, to: fixture.root.appendingPathComponent("absent.raw").path)
        await FirstRunOptionalRecoveryFixture.settle(host)

        let scroll = try XCTUnwrap(fileField.enclosingScrollView)
        let document = try XCTUnwrap(scroll.documentView)
        let buttonHeight = NSButton(title: "가져오기", target: nil, action: nil).fittingSize.height
        func footerHeight() -> CGFloat {
            scroll.convert(scroll.bounds, to: nil).minY - window.contentLayoutRect.minY
        }

        XCTAssertGreaterThan(document.bounds.height, scroll.contentView.bounds.height)
        let initialHeight = footerHeight()
        XCTAssertGreaterThanOrEqual(initialHeight, buttonHeight)
        let bottom = document.isFlipped ? document.bounds.maxY - scroll.contentView.bounds.height : document.bounds.minY
        scroll.contentView.scroll(to: NSPoint(x: 0, y: bottom))
        scroll.reflectScrolledClipView(scroll.contentView)
        await FirstRunOptionalRecoveryFixture.settle(host)
        XCTAssertEqual(footerHeight(), initialHeight)
        let lastField = try XCTUnwrap(fields.last)
        let lastFieldRect = lastField.convert(lastField.bounds, to: document)
        XCTAssertTrue(scroll.documentVisibleRect.intersects(lastFieldRect))

        try await FirstRunOptionalRecoveryFixture.submit(host, library: library)
        XCTAssertNotNil(library.firstRunImportError)
        XCTAssertTrue(VMLibrary.list(rootURL: fixture.library).isEmpty)
    }
}
