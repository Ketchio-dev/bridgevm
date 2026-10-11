import XCTest
@testable import BridgeVMControl

final class HvfPendingDeletionJournalTests: XCTestCase {
    func testUnpublishedFinalizationPairAndJournalAreNotDeleted() throws {
        let fixture = try HvfWindowsInstallStagingFixture()
        defer { fixture.remove() }
        try fixture.register()
        var config = try JSONDecoder().decode(VMConfig.self, from: Data(contentsOf: fixture.configURL))
        config.bootMode = "windows-hvf"
        config.diskPath = fixture.finalDisk.path
        XCTAssertTrue(VMLibrary.save(config, rootURL: fixture.library))
        _ = try fixture.writeInstallerOutputs(log: Data("preserved synthetic install evidence".utf8))
        XCTAssertThrowsError(try fixture.finalize(stoppingAt: .varsStaged))
        let fm = FileManager.default
        XCTAssertFalse(fm.fileExists(atPath: fixture.finalDisk.path))
        XCTAssertFalse(fm.fileExists(atPath: fixture.bundle.appendingPathComponent("metadata/hvf-vars.fd").path))
        let journal = try fixture.readJournal()
        XCTAssertEqual(journal.phase, .varsStaged)
        let before = try snapshot(fixture.root)
        XCTAssertGreaterThan(before.count, 5)
        let journalBytes = try Data(contentsOf: fixture.journalURL)
        XCTAssertFalse(LibraryDeletionProtection.delete(config, rootURL: fixture.library))
        XCTAssertEqual(try snapshot(fixture.root), before)
        XCTAssertEqual(try Data(contentsOf: fixture.journalURL), journalBytes)
    }

    private func snapshot(_ root: URL) throws -> [String: Data] {
        let iterator = try XCTUnwrap(FileManager.default.enumerator(at: root,
            includingPropertiesForKeys: [.isRegularFileKey, .isSymbolicLinkKey]))
        var files: [String: Data] = [:]
        for case let path as URL in iterator {
            let values = try path.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
            XCTAssertFalse(values.isSymbolicLink == true)
            if values.isRegularFile == true { files[path.path] = try Data(contentsOf: path) }
        }
        return files
    }
}
