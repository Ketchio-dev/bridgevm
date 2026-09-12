import XCTest
@testable import BridgeVMControl

final class VMRelocationRecoveryTests: XCTestCase {
    private final class FailingRollback: FileManager, @unchecked Sendable {
        var moves = 0
        override func moveItem(at source: URL, to destination: URL) throws {
            moves += 1
            if moves == 2 { throw CocoaError(.fileWriteNoPermission) }
            try super.moveItem(at: source, to: destination)
        }
    }

    private final class Fixture {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        var library: URL { root.appendingPathComponent("library") }
        var source: URL { library.appendingPathComponent("move-fixture/bundle.vmbridge") }
        var parent: URL { root.appendingPathComponent("moved") }
        var destination: URL { parent.appendingPathComponent("bundle.vmbridge") }
        var config: VMConfig {
            VMConfig(id: "move-fixture", name: "Move", displayName: "Move",
                backendKind: "hvf-engine", bootMode: nil, bundlePath: source.path,
                runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
                sshUser: "", leasesPath: "", guestName: "move-fixture", displayWidth: 800, displayHeight: 600)
        }
        init() throws {
            try FileManager.default.createDirectory(at: source.appendingPathComponent("metadata/vtpm"), withIntermediateDirectories: true)
            try Data("unchanged-media".utf8).write(to: source.appendingPathComponent("disk"))
            // A regular file makes lifecycle receipt directory creation fail.
            try Data("blocked".utf8).write(to: source.appendingPathComponent("metadata/vtpm-lifecycle"))
            XCTAssertTrue(VMLibrary.save(config, rootURL: library))
        }
        deinit { try? FileManager.default.removeItem(at: root) }
        func assertRegistered(at bundle: URL) throws {
            XCTAssertEqual(VMLibrary.list(rootURL: library).first?.bundlePath, bundle.path)
            XCTAssertEqual(try Data(contentsOf: bundle.appendingPathComponent("disk")), Data("unchanged-media".utf8))
        }
    }

    func testSuccessfulRollbackRestoresFilesBeforeOriginalRegistration() throws {
        let fixture = try Fixture()
        XCTAssertNil(VMLibrary.moveWindowsHVFBundle(fixture.config, to: fixture.parent, rootURL: fixture.library))
        try fixture.assertRegistered(at: fixture.source)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.destination.path))
    }

    func testFailedRollbackRetainsRegistrationAtExistingDestination() throws {
        let fixture = try Fixture()
        let manager = FailingRollback()
        XCTAssertNil(VMLibrary.moveWindowsHVFBundle(fixture.config, to: fixture.parent,
            rootURL: fixture.library, fileManager: manager))
        XCTAssertEqual(manager.moves, 2)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.source.path))
        try fixture.assertRegistered(at: fixture.destination)
    }

    func testDescendantDestinationDoesNotCreateDirectoriesInsideSource() throws {
        let fixture = try Fixture()
        let nested = fixture.source.appendingPathComponent("must-not-exist")
        XCTAssertNil(VMLibrary.moveWindowsHVFBundle(fixture.config, to: nested, rootURL: fixture.library))
        XCTAssertFalse(FileManager.default.fileExists(atPath: nested.path))
        try fixture.assertRegistered(at: fixture.source)
    }

    func testSymlinkDescendantDestinationIsRejectedBeforeCreation() throws {
        let fixture = try Fixture()
        let alias = fixture.root.appendingPathComponent("alias")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: fixture.source)
        XCTAssertNil(VMLibrary.moveWindowsHVFBundle(fixture.config,
            to: alias.appendingPathComponent("must-not-exist"), rootURL: fixture.library))
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.source.appendingPathComponent("must-not-exist").path))
        try fixture.assertRegistered(at: fixture.source)
    }
}
