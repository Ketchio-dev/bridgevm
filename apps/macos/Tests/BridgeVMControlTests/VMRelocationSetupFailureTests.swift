import Foundation
import XCTest
@testable import BridgeVMControl

final class VMRelocationSetupFailureTests: XCTestCase {
    private final class Refusal: FileManager, @unchecked Sendable {
        var moves = 0
        override func createDirectory(at url: URL, withIntermediateDirectories createIntermediates: Bool,
                                      attributes: [FileAttributeKey: Any]? = nil) throws {
            throw CocoaError(.fileWriteNoPermission)
        }
        override func moveItem(at source: URL, to destination: URL) throws {
            moves += 1
            throw CocoaError(.fileWriteUnknown)
        }
    }

    func testParentCreationFailureDoesNotBlockUnmovedVM() throws {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        addTeardownBlock { try? fm.removeItem(at: root) }
        let library = root.appendingPathComponent("library")
        let source = root.appendingPathComponent("source/bundle.vmbridge")
        try fm.createDirectory(at: source, withIntermediateDirectories: true)
        let config = VMConfig(id: "setup-fixture", name: "Setup Fixture",
            displayName: "Setup Fixture", backendKind: "hvf-engine",
            bundlePath: source.path, runnerPath: "", launchSpecPath: "",
            handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
            guestName: "Windows", displayWidth: 800, displayHeight: 600)
        XCTAssertTrue(VMLibrary.save(config, rootURL: library))
        let faults = Refusal()
        let parent = root.appendingPathComponent("destination")
        XCTAssertNil(VMLibrary.moveWindowsHVFBundle(config, to: parent,
                                                   rootURL: library, fileManager: faults))
        XCTAssertEqual(faults.moves, 0)
        XCTAssertFalse(VMRelocationJournal.isPending(config, rootURL: library))
        XCTAssertEqual(VMLibrary.list(rootURL: library).first?.bundlePath, source.path)
        XCTAssertTrue(fm.fileExists(atPath: source.path))
        XCTAssertFalse(fm.fileExists(atPath: parent.path))
    }
}
