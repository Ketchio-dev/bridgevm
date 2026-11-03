import SwiftUI
import XCTest
@testable import BridgeVMControl

/// The import form's choosers use the asynchronous panel, like the create sheet (-25204 under runModal).
@MainActor
final class LibraryFileFieldTests: XCTestCase {
    func testChooserReturnsBeforeTheUserChoosesAndCancelKeepsThePath() {
        var path = "/typed/windows.raw"
        let binding = Binding<String>(get: { path }, set: { path = $0 })

        let file = SelectionPanelStub()
        LibraryFileField.choose(with: file, directory: false, path: binding)
        XCTAssertEqual(file.beginCalls, 1)
        XCTAssertTrue(file.canChooseFiles)
        XCTAssertFalse(file.canChooseDirectories)
        XCTAssertFalse(file.allowsMultipleSelection)
        XCTAssertEqual(path, "/typed/windows.raw")
        file.url = URL(fileURLWithPath: "/chosen/windows.raw")
        file.complete(.OK)
        XCTAssertEqual(path, "/chosen/windows.raw")

        let folder = SelectionPanelStub()
        LibraryFileField.choose(with: folder, directory: true, path: binding)
        XCTAssertTrue(folder.canChooseDirectories)
        XCTAssertFalse(folder.canChooseFiles)
        folder.url = URL(fileURLWithPath: "/canceled/vtpm", isDirectory: true)
        folder.complete(.cancel)
        XCTAssertEqual(path, "/chosen/windows.raw")
    }
}
