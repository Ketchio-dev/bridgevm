import SwiftUI
import XCTest
@testable import BridgeVMControl

/// The import form's choosers use the asynchronous panel, like the create sheet (-25204 under runModal).
@MainActor
final class LibraryFileFieldTests: XCTestCase {
    func testDroppingMediaReplacesOnlyOneMatchingFileOrFolder() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: folder) }
        let disk = folder.appendingPathComponent("설치된 Windows.raw")
        try Data([0]).write(to: disk)
        var path = ""
        let selection = Binding(get: { path }, set: { path = $0 })

        XCTAssertTrue(LibraryFileField.selectDrop([disk], directory: false, path: selection))
        XCTAssertEqual(path, disk.path)
        XCTAssertFalse(LibraryFileField.selectDrop([folder], directory: false, path: selection))
        XCTAssertFalse(LibraryFileField.selectDrop([disk, disk], directory: false, path: selection))
        XCTAssertFalse(LibraryFileField.selectDrop([try XCTUnwrap(URL(string: "https://example.org/disk.raw"))],
                                                  directory: false, path: selection))
        var remoteFile = try XCTUnwrap(URLComponents(url: disk, resolvingAgainstBaseURL: false))
        remoteFile.host = "other-mac.local"
        XCTAssertFalse(LibraryFileField.selectDrop([try XCTUnwrap(remoteFile.url)], directory: false, path: selection))
        XCTAssertEqual(path, disk.path)
        XCTAssertTrue(LibraryFileField.selectDrop([folder], directory: true, path: selection))
        XCTAssertFalse(LibraryFileField.selectDrop([disk], directory: true, path: selection))
        XCTAssertEqual(path, folder.path)
    }

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
