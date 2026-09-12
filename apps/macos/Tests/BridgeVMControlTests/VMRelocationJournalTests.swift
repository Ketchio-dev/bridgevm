import Foundation
import XCTest
@testable import BridgeVMControl

final class VMRelocationJournalTests: XCTestCase {
    private final class Faults: FileManager, @unchecked Sendable {
        let registration: URL
        let partial: Bool
        var moves = 0
        init(registration: URL, partial: Bool) {
            self.registration = registration
            self.partial = partial
            super.init()
        }
        override func moveItem(at source: URL, to destination: URL) throws {
            moves += 1
            if partial && moves == 1 {
                try createDirectory(at: destination, withIntermediateDirectories: true)
                throw CocoaError(.fileWriteUnknown)
            }
            if moves == 2 {
                try setAttributes([.posixPermissions: 0o500], ofItemAtPath: registration.path)
            }
            try super.moveItem(at: source, to: destination)
        }
    }

    private func fixture() throws -> (root: URL, library: URL, config: VMConfig) {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let library = root.appendingPathComponent("library")
        let source = root.appendingPathComponent("source/bundle.vmbridge")
        let config = VMConfig(id: "journal-fixture", name: "Journal Fixture",
            displayName: "Journal Fixture", backendKind: "hvf-engine",
            bundlePath: source.path, runnerPath: "", launchSpecPath: "",
            handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
            guestName: "Windows", displayWidth: 800, displayHeight: 600)
        addTeardownBlock {
            try? fm.setAttributes([.posixPermissions: 0o700],
                ofItemAtPath: library.appendingPathComponent(config.slug).path)
            try? fm.removeItem(at: root)
        }
        try fm.createDirectory(at: source.appendingPathComponent("metadata/vtpm"),
                               withIntermediateDirectories: true)
        try Data("force-receipt-failure".utf8).write(to: source.appendingPathComponent("metadata/vtpm-lifecycle"))
        try Data("retained-media".utf8).write(to: source.appendingPathComponent("disk.raw"))
        XCTAssertTrue(VMLibrary.save(config, rootURL: library))
        return (root, library, config)
    }

    func testPendingRecordExcludesRegisteredVMAndCannotBeOverwritten() throws {
        let f = try fixture()
        XCTAssertEqual(VMLibrary.list(rootURL: f.library).count, 1)
        var moved = f.config
        moved.bundlePath = f.root.appendingPathComponent("target/bundle.vmbridge").path
        let pending = try VMRelocationJournal.begin(original: f.config, moved: moved, rootURL: f.library)
        let before = try Data(contentsOf: pending)
        XCTAssertThrowsError(try VMRelocationJournal.begin(original: f.config, moved: moved, rootURL: f.library))
        XCTAssertEqual(try Data(contentsOf: pending), before)
        XCTAssertTrue(VMLibrary.list(rootURL: f.library).isEmpty)
        XCTAssertFalse(VMLibrary.scan(rootURL: f.library).issues.isEmpty)
        try Data("incomplete-record".utf8).write(to: pending)
        XCTAssertTrue(VMLibrary.list(rootURL: f.library).isEmpty)
    }

    func testFailedRecoverySaveRetainsRecordAndExcludesStaleRegistration() throws {
        let f = try fixture()
        let registration = f.library.appendingPathComponent(f.config.slug)
        let faults = Faults(registration: registration, partial: false)
        let target = f.root.appendingPathComponent("target")
        XCTAssertNil(VMLibrary.moveWindowsHVFBundle(f.config, to: target,
                                                   rootURL: f.library, fileManager: faults))
        XCTAssertEqual(faults.moves, 2)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: registration.path)
        let stored = try JSONDecoder().decode(VMConfig.self,
            from: Data(contentsOf: registration.appendingPathComponent("vm.json")))
        XCTAssertEqual(stored.bundlePath, target.appendingPathComponent("bundle.vmbridge").path)
        XCTAssertFalse(FileManager.default.fileExists(atPath: stored.bundlePath))
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: f.config.bundlePath).appendingPathComponent("disk.raw")),
                       Data("retained-media".utf8))
        XCTAssertTrue(VMRelocationJournal.isPending(f.config, rootURL: f.library))
        XCTAssertTrue(VMLibrary.list(rootURL: f.library).isEmpty)
        XCTAssertFalse(VMLibrary.scan(rootURL: f.library).issues.isEmpty)
    }

    func testInitialPartialMoveRetainsBothLocationsAndBlocksEntry() throws {
        let f = try fixture()
        let faults = Faults(registration: f.library.appendingPathComponent(f.config.slug), partial: true)
        let target = f.root.appendingPathComponent("target")
        XCTAssertNil(VMLibrary.moveWindowsHVFBundle(f.config, to: target,
                                                   rootURL: f.library, fileManager: faults))
        XCTAssertEqual(faults.moves, 1)
        XCTAssertTrue(FileManager.default.fileExists(atPath: f.config.bundlePath))
        XCTAssertTrue(FileManager.default.fileExists(atPath: target.appendingPathComponent("bundle.vmbridge").path))
        XCTAssertTrue(VMRelocationJournal.isPending(f.config, rootURL: f.library))
        XCTAssertTrue(VMLibrary.list(rootURL: f.library).isEmpty)
    }
}
