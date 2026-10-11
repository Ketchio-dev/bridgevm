import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeCLICreateWindowsNameTests: XCTestCase {
    func testCreateWindowsLiteralAfterTheCommandIsTheVMName() throws {
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let iso = root.appendingPathComponent("Windows.iso").path
        for arguments in [["create-windows", "create-windows", "--iso", iso, "--cpus", "1"],
                          ["--json", "create-windows", "--iso", iso, "--cpus", "1", "create-windows"]] {
            XCTAssertTrue(NativeCLICreateWindowsParser.selectsCommand(arguments))
            let parsed = try NativeCLIOptions.parse(arguments: arguments, defaultLibrary: root)
            guard case .createWindows(let request) = parsed.command else { return XCTFail("wrong command") }
            XCTAssertEqual(request.name, "create-windows")
            XCTAssertFalse(parsed.showHelp)
        }
    }

    func testCreateWindowsLiteralAfterANameIsADuplicateName() throws {
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let iso = root.appendingPathComponent("Windows.iso").path
        for arguments in [["create-windows", "VM", "create-windows", "--iso", iso],
                          ["create-windows", "create-windows", "create-windows", "--iso", iso]] {
            XCTAssertThrowsError(try NativeCLICreateWindowsParser.parse(
                arguments: arguments, defaultLibrary: root, hostCPU: 8)) { error in
                XCTAssertEqual(error.localizedDescription,
                               "Unknown or duplicate create-windows argument. Use --cli --help.")
            }
        }
    }

    private func makeRoot() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try Data("synthetic-iso".utf8).write(to: root.appendingPathComponent("Windows.iso"))
        return root
    }
}
