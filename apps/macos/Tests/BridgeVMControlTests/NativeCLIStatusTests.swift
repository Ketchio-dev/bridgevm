import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeCLIStatusTests: XCTestCase {
    func testStatusOptionsPreserveUnicodeWithoutRequiringRegistration() throws {
        for arguments in [["status", "개발-vm", "--json"], ["--json", "status", "개발-vm"]] {
            let options = try NativeCLIOptions.parse(arguments: arguments)
            XCTAssertEqual(options.command, .status("개발-vm"))
            XCTAssertTrue(options.json)
        }
        XCTAssertTrue(try NativeCLIOptions.parse(arguments: ["status", "--help"]).showHelp)
        for arguments in [["status"], ["status", "../vm"], ["status", "vm", "extra"]] {
            XCTAssertThrowsError(try NativeCLIOptions.parse(arguments: arguments))
        }
    }

    func testAbsentOwnerReturnsStructuredUnobservedWithoutCreatingLibrary() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("native-status-\(UUID().uuidString)")
        let snapshot = NativeCLIRuntimeStatus.snapshot(rootURL: root, id: "missing")
        XCTAssertFalse(snapshot.complete)
        XCTAssertEqual(snapshot.runtimeState, "unobserved")
        XCTAssertEqual(snapshot.unavailableReason, "ownerUnavailable")
        XCTAssertTrue(snapshot.sessions.isEmpty)
        XCTAssertNil(snapshot.appInstanceID)
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.path))
    }

    func testMissingAndUnreadableSavedConfigurationDoNotMutateLibrary() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("native-status-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let handle = try NativeRuntimeLibraryHandle.open(rootURL: root, create: false)
        XCTAssertEqual(NativeCLIStatusConfiguration.read(library: handle, id: "missing").state, .missing)
        let entry = root.appendingPathComponent("broken")
        try FileManager.default.createDirectory(at: entry, withIntermediateDirectories: true)
        let bytes = Data("invalid config".utf8), config = entry.appendingPathComponent("vm.json")
        try bytes.write(to: config)
        let saved = NativeCLIStatusConfiguration.read(library: handle, id: "broken")
        XCTAssertEqual(saved.state, .unreadable)
        XCTAssertNil(saved.digest)
        XCTAssertEqual(try Data(contentsOf: config), bytes)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), ["broken"])
    }
}
