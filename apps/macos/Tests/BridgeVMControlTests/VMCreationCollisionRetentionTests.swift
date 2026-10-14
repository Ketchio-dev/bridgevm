import XCTest
@testable import BridgeVMControl

final class VMCreationCollisionRetentionTests: XCTestCase {
    private func collideThenMoveWinner(_ config: VMConfig, root: URL, archive: URL) -> VMRegistrationCommitOutcome {
        var winner = config
        winner.displayName = "Earlier committed registration"
        XCTAssertTrue(VMLibrary.createOutcome(winner, rootURL: root).isCommitted)
        let refused = VMLibrary.createOutcome(config, rootURL: root)
        guard case .notPublished(let error) = refused, error is VMRegistrationCollision else {
            XCTFail("expected actual exclusive EEXIST refusal")
            return refused
        }
        do {
            try FileManager.default.moveItem(at: root.appendingPathComponent(config.slug + "/vm.json"), to: archive)
        } catch { XCTFail("move winner after failed publication: \(error)") }
        return refused
    }

    func testFirstRunKeepsPreparedMediaAfterConflictingRegistrationMovesAway() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let archive = fixture.root.appendingPathComponent("winner.json")
        let result = FirstRunImportWorker.run(fixture.inputs(), libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper) { config, root in
            collideThenMoveWinner(config, root: root, archive: archive)
        }
        guard case .failed = result else { return XCTFail("collision must remain a failed import") }
        XCTAssertFalse(NativeLibraryReader.entryMayExist(fixture.entry.appendingPathComponent("vm.json")))
        XCTAssertEqual(try Data(contentsOf: fixture.entry.appendingPathComponent("bundle/disks/hvf-target.raw")),
            try Data(contentsOf: fixture.disk))
        XCTAssertEqual(try Data(contentsOf: fixture.entry.appendingPathComponent("bundle/metadata/hvf-vars.fd")),
            try Data(contentsOf: fixture.vars))
        XCTAssertEqual(try JSONDecoder().decode(VMConfig.self, from: Data(contentsOf: archive)).displayName,
            "Earlier committed registration")
        try fixture.assertSources()
    }

    func testBothHVFFactoriesKeepExternalPreparationAfterConflictingRegistrationMovesAway() throws {
        let source = try FirstRunImportTransactionFixture()
        defer { source.remove() }
        for importing in [true, false] {
            let fixture = try VMCreationRegistrationCollisionTests.Fixture()
            defer { fixture.remove() }
            let archive = fixture.root.appendingPathComponent("winner.json")
            var prepared: VMConfig?
            let publish: (VMConfig, URL) -> VMRegistrationCommitOutcome = { config, root in
                prepared = config
                return self.collideThenMoveWinner(config, root: root, archive: archive)
            }
            let outcome = importing
                ? VMLibrary.createWindowsHVF(name: "Collision", targetDiskPath: source.disk.path,
                    varsPath: source.vars.path, storageDir: fixture.storage, libraryRoot: fixture.library,
                    snapshotHelper: HvfMediaImportTestSupport.helper, publish: publish)
                : fixture.createExternal(publish: publish)
            XCTAssertNil(outcome, "preservation must not advertise a created VM")
            let config = try XCTUnwrap(prepared)
            XCTAssertFalse(NativeLibraryReader.entryMayExist(fixture.library.appendingPathComponent(config.slug + "/vm.json")))
            XCTAssertTrue(FileManager.default.fileExists(atPath: config.bundlePath))
            XCTAssertTrue(FileManager.default.fileExists(atPath: archive.path))
            if importing {
                let handle = try FileHandle(forReadingFrom: URL(fileURLWithPath: XCTUnwrap(config.diskPath)))
                defer { try? handle.close() }
                XCTAssertEqual(try handle.read(upToCount: 4), Data([1, 2, 3, 4]))
            } else { XCTAssertEqual(try fixture.ownedISO(config), fixture.guiBytes) }
            try source.assertSources()
        }
    }

    func testBooleanCreationRefusesCollisionButTransfersPreservation() throws {
        let fixture = try VMCreationRegistrationCollisionTests.Fixture()
        defer { fixture.remove() }
        guard case .created(let config)? = fixture.createExternal() else { return XCTFail("prepare winner") }
        var preserved = false
        XCTAssertFalse(VMLibrary.createRegistration(config, rootURL: fixture.library, preserving: { preserved = true }))
        XCTAssertTrue(preserved)
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: fixture.library, id: config.slug), config)
    }
}
