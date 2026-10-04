import AppKit
import SwiftUI
import UniformTypeIdentifiers
import XCTest
@testable import BridgeVMControl

@MainActor
final class FileSelectionTests: XCTestCase {
    func testSelectionRemainsPendingUntilTheAsynchronousPanelCompletes() {
        let panel = SelectionPanelStub()
        var selected: URL?
        FileSelection.present(panel, directories: false, extensions: ["iso", "img"]) { selected = $0 }
        XCTAssertEqual(panel.beginCalls, 1)
        XCTAssertNil(selected)
        XCTAssertFalse(panel.allowsMultipleSelection)
        XCTAssertTrue(panel.canChooseFiles)
        XCTAssertFalse(panel.canChooseDirectories)
        XCTAssertEqual(panel.allowedContentTypes, ["iso", "img"].compactMap { UTType(filenameExtension: $0) })
        panel.url = URL(fileURLWithPath: "/tmp/selected.iso")
        panel.complete(.OK)
        XCTAssertEqual(selected, panel.url)
    }

    func testCancelPreservesThePreviousSelectionEvenIfThePanelHasAURL() {
        let panel = SelectionPanelStub()
        let original = "/tmp/original share"
        var selected = original
        let text = Binding<String>(get: { selected }, set: { selected = $0 })
        FileSelection.present(panel, directories: true) { text.wrappedValue = $0.path }
        panel.url = URL(fileURLWithPath: "/tmp/canceled share", isDirectory: true)
        panel.complete(.cancel)
        XCTAssertEqual(selected, original)
    }

    func testAnOKResponseWithoutAURLDoesNotInventASelection() {
        let panel = SelectionPanelStub()
        let original = "/tmp/original share"
        var selected = original
        let text = Binding<String>(get: { selected }, set: { selected = $0 })
        FileSelection.present(panel, directories: true) { text.wrappedValue = $0.path }
        panel.complete(.OK)
        XCTAssertEqual(selected, original)
    }

    func testDirectorySelectionExcludesFilesAndAcceptsTheChosenDirectory() {
        let panel = SelectionPanelStub()
        let original = "/tmp/original share"
        var selected = original
        let text = Binding<String>(get: { selected }, set: { selected = $0 })
        FileSelection.present(panel, directories: true) { text.wrappedValue = $0.path }
        XCTAssertEqual(panel.beginCalls, 1)
        XCTAssertEqual(text.wrappedValue, original)
        XCTAssertTrue(panel.canChooseDirectories)
        XCTAssertFalse(panel.canChooseFiles)
        XCTAssertFalse(panel.allowsMultipleSelection)
        XCTAssertTrue(panel.allowedContentTypes.isEmpty)
        let chosen = "/tmp/선택한 shared folder"
        panel.url = URL(fileURLWithPath: chosen, isDirectory: true)
        XCTAssertEqual(text.wrappedValue, original)
        panel.complete(.OK)
        XCTAssertEqual(text.wrappedValue, chosen)
    }
}
