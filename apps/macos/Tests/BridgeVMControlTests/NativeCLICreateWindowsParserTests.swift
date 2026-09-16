import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeCLICreateWindowsParserTests: XCTestCase {
    func testParsesExactUnicodeRequestAndAllProductOptions() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let arguments = ["--json", "create-windows", "개발 VM", "--iso", fixture.iso.path,
                         "--disk-gib", "128", "--memory-mib", "8192", "--cpus", "6",
                         "--resolution", "1920x1080", "--no-network", "--library", fixture.library.path]
        let parsed = try NativeCLICreateWindowsParser.parse(
            arguments: arguments, defaultLibrary: fixture.root, hostCPU: 12)
        guard case .createWindows(let request) = parsed.command else { return XCTFail("wrong command") }
        XCTAssertEqual(request.name, "개발 VM")
        XCTAssertEqual(request.isoPath, fixture.iso.path)
        XCTAssertEqual(request.diskGiB, 128)
        XCTAssertEqual(request.memoryMiB, 8_192)
        XCTAssertEqual(request.cpuCount, 6)
        XCTAssertEqual(request.resolution.text, "1920x1080")
        XCTAssertFalse(request.networkEnabled)
        XCTAssertEqual(parsed.libraryRoot, fixture.library)
        XCTAssertTrue(parsed.json)
    }

    func testDefaultsMatchTheCreateSheetProductDefaults() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let parsed = try NativeCLICreateWindowsParser.parse(
            arguments: ["create-windows", "VM", "--iso", fixture.iso.path],
            defaultLibrary: fixture.library, hostCPU: 8)
        guard case .createWindows(let request) = parsed.command else { return XCTFail("wrong command") }
        XCTAssertEqual(request.diskGiB, 64)
        XCTAssertEqual(request.memoryMiB, 6_144)
        XCTAssertEqual(request.cpuCount, 4)
        XCTAssertEqual(request.resolution.text, "1440x900")
        XCTAssertTrue(request.networkEnabled)
    }

    func testRejectsDuplicateUnsafeMissingAndHostInvalidInputs() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let directory = fixture.root.appendingPathComponent("directory", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let link = fixture.root.appendingPathComponent("linked.iso")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: fixture.iso)
        let invalid = [
            ["create-windows", "VM", "--iso", "relative.iso"],
            ["create-windows", "VM", "--iso", fixture.root.appendingPathComponent("missing.iso").path],
            ["create-windows", "VM", "--iso", directory.path],
            ["create-windows", "VM", "--iso", link.path],
            ["create-windows", " VM ", "--iso", fixture.iso.path],
            ["create-windows", "VM", "--iso", fixture.iso.path, "--iso", fixture.iso.path],
            ["create-windows", "VM", "--iso", fixture.iso.path, "--cpus", "8"],
            ["create-windows", "VM", "--iso", fixture.iso.path, "--disk-gib", "65"],
        ]
        for arguments in invalid {
            XCTAssertThrowsError(try NativeCLICreateWindowsParser.parse(
                arguments: arguments, defaultLibrary: fixture.library, hostCPU: 8), "\(arguments)")
        }
    }

    func testHelpDoesNotReadMediaOrCreateTheLibrary() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let library = root.appendingPathComponent("absent-library")
        let parsed = try NativeCLICreateWindowsParser.parse(
            arguments: ["--library", library.path, "create-windows", "--help"], defaultLibrary: root)
        XCTAssertTrue(parsed.showHelp)
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.path))
    }

    func testCreateWindowsCanRemainAnExistingVMIDForOtherCommands() throws {
        let parsed = try NativeCLIOptions.parse(arguments: ["inspect", "create-windows"])
        XCTAssertEqual(parsed.command, .inspect("create-windows"))
        XCTAssertTrue(NativeCLICreateWindowsParser.selectsCommand(
            ["--library", "/tmp/library", "--json", "create-windows", "VM", "--iso", "/tmp/a.iso"]))
    }

    private struct Fixture {
        let root: URL
        let library: URL
        let iso: URL
        init() throws {
            root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
            library = root.appendingPathComponent("library", isDirectory: true)
            iso = root.appendingPathComponent("Windows 11.iso")
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            try Data("synthetic-iso".utf8).write(to: iso)
        }
        func remove() { try? FileManager.default.removeItem(at: root) }
    }
}
