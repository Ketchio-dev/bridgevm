import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeCLIOptionsTests: XCTestCase {
    private let library = URL(fileURLWithPath: "/private/tmp/native-cli-options-absent-library", isDirectory: true)

    func testListUsesInjectedLibraryAndDefaultOutputWithoutAccessingIt() throws {
        let options = try NativeCLIOptions.parse(arguments: ["list"], defaultLibrary: library)
        if case .list = options.command {} else { XCTFail("Expected list command") }
        XCTAssertEqual(options.libraryRoot, library)
        XCTAssertFalse(options.json)
        XCTAssertFalse(options.showHelp)
    }

    func testInspectPreservesNativeUnicodeIDWithFlagsInAnyOrder() throws {
        let variants = [
            ["inspect", "개발-vm", "--json", "--library", library.path],
            ["--library", library.path, "--json", "inspect", "개발-vm"],
            ["inspect", "--json", "개발-vm", "--library", library.path]
        ]
        for arguments in variants {
            let options = try NativeCLIOptions.parse(arguments: arguments)
            if case let .inspect(id) = options.command { XCTAssertEqual(id, "개발-vm") }
            else { XCTFail("Expected inspect command") }
            XCTAssertEqual(options.libraryRoot, library)
            XCTAssertTrue(options.json)
            XCTAssertFalse(options.showHelp)
        }
    }

    func testHelpNeedsNoExistingLibrary() throws {
        let parent = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let absent = parent.appendingPathComponent("not-created")
        for arguments in [["--help"], ["-h"], ["list", "--help"], ["--help", "--library", absent.path]] {
            let options = try NativeCLIOptions.parse(arguments: arguments, defaultLibrary: absent)
            XCTAssertTrue(options.showHelp)
            XCTAssertFalse(FileManager.default.fileExists(atPath: parent.path))
        }
    }

    func testRejectsUnknownDuplicateRelativeAndIncompleteArguments() {
        let invalid = [
            ["unsupported", "vm"], ["list", "extra"], ["inspect"], ["inspect", "a", "b"],
            ["list", "--unknown"], ["list", "--json", "--json"],
            ["list", "--library"], ["list", "--library", "relative/library"],
            ["list", "--library", ""],
            ["list", "--library", library.path, "--library", library.path]
        ]
        for arguments in invalid {
            XCTAssertThrowsError(try NativeCLIOptions.parse(arguments: arguments, defaultLibrary: library),
                                 "Invalid arguments were accepted: \(arguments)")
        }
    }
}
