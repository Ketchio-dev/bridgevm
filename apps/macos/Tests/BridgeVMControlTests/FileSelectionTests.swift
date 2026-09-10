import AppKit
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
        let original = URL(fileURLWithPath: "/tmp/original.iso")
        var selected = original
        FileSelection.present(panel, directories: false) { selected = $0 }
        panel.url = URL(fileURLWithPath: "/tmp/canceled.iso")
        panel.complete(.cancel)
        XCTAssertEqual(selected, original)
    }

    func testAnOKResponseWithoutAURLDoesNotInventASelection() {
        let panel = SelectionPanelStub()
        var called = false
        FileSelection.present(panel, directories: false) { _ in called = true }
        panel.complete(.OK)
        XCTAssertFalse(called)
    }

    func testDirectorySelectionExcludesFilesAndAcceptsTheChosenDirectory() {
        let panel = SelectionPanelStub()
        var selected: URL?
        FileSelection.present(panel, directories: true) { selected = $0 }
        XCTAssertTrue(panel.canChooseDirectories)
        XCTAssertFalse(panel.canChooseFiles)
        XCTAssertFalse(panel.allowsMultipleSelection)
        panel.url = URL(fileURLWithPath: "/tmp/payload", isDirectory: true)
        panel.complete(.OK)
        XCTAssertEqual(selected, panel.url)
    }
}

@MainActor
private final class SelectionPanelStub: FileSelectionPanel {
    var allowsMultipleSelection = true
    var canChooseDirectories = false
    var canChooseFiles = true
    var allowedContentTypes: [UTType] = []
    var url: URL?
    var beginCalls = 0
    private var handler: ((NSApplication.ModalResponse) -> Void)?

    func begin(completionHandler handler: @escaping (NSApplication.ModalResponse) -> Void) {
        beginCalls += 1
        self.handler = handler
    }

    func complete(_ response: NSApplication.ModalResponse) {
        let callback = handler
        handler = nil
        callback?(response)
    }
}
