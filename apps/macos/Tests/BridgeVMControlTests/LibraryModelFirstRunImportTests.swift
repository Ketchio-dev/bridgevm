import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryModelFirstRunImportTests: XCTestCase {
    /// Cancel was missing: a long copy of the wrong disk could only be force quit, which left the
    /// helper running and its private workspace (tens of GB) on the drive.
    func testCancelStopsTheHelperAndLeavesNoVMOrWorkspace() async throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let marker = fixture.root.appendingPathComponent("helper-started")
        let helper = fixture.root.resolvingSymlinksInPath().appendingPathComponent("snapshot_pair_cli")
        // A slow `create`: a partial copy in the workspace, then a wait as long as a big disk's copy.
        try Data("""
            #!/bin/sh
            mkdir -p "$4" && head -c 4096 /dev/zero > "$4/disk.raw" && cp "$2" "$4/vars.fd"
            echo "$$ $4" > "\(marker.path)"
            exec /bin/sleep 20
            """.utf8).write(to: helper)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: helper.path)
        let model = LibraryModel(rootURL: fixture.library, migrateLegacy: false)
        let task = Task { await model.importExistingHvfVM(fixture.inputs(), snapshotHelper: helper) }
        var started: [String] = []
        for _ in 0..<200 where started.count < 2 {
            try await Task.sleep(nanoseconds: 50_000_000)
            started = ((try? String(contentsOf: marker, encoding: .utf8)) ?? "").split(separator: " ")
                .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
        }
        let pid = try XCTUnwrap(Int32(started.first ?? ""))
        let workspace = URL(fileURLWithPath: started[1]).deletingLastPathComponent()
        XCTAssertTrue(model.firstRunImportBusy)
        XCTAssertEqual(model.firstRunImport.stage?.canCancel, true)
        let duplicate = await model.importExistingHvfVM(fixture.inputs(), worker: { _, _, _, _ in
            XCTFail("busy model must not start another worker")
            return .failed("duplicate")
        })
        XCTAssertNotNil(duplicate)

        let clicked = Date()
        model.firstRunImport.cancelHandle?.cancel()  // what the Cancel button calls
        let error = await task.value
        XCTAssertLessThan(Date().timeIntervalSince(clicked), 10)
        XCTAssertNil(error)
        XCTAssertNil(model.firstRunImportError)
        XCTAssertFalse(model.firstRunImportBusy)
        XCTAssertTrue(model.vms.isEmpty)
        XCTAssertTrue(VMLibrary.list(rootURL: fixture.library).isEmpty)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: workspace.path))
        XCTAssertNotEqual(kill(pid, 0), 0, "the helper must not outlive the import")
        try fixture.assertSources()
    }

    func testCancellationBeforeAcceptanceDoesNotInvokeWorker() async throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let model = LibraryModel(rootURL: fixture.library, migrateLegacy: false)
        let task = Task {
            await model.importExistingHvfVM(fixture.inputs(), worker: { _, _, _, _ in
                XCTFail("already cancelled request must not invoke worker")
                return .failed("unexpected")
            })
        }
        task.cancel()
        let error = await task.value
        XCTAssertNotNil(error)
        XCTAssertFalse(model.firstRunImportBusy)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path))
    }

    func testAdoptionFailurePreservesSuccessfullyPublishedImport() async throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let model = LibraryModel(rootURL: fixture.library, migrateLegacy: false)
        let error = await model.importExistingHvfVM(fixture.inputs(),
            snapshotHelper: HvfMediaImportTestSupport.helper, adopt: { _ in false })
        XCTAssertNotNil(error)
        XCTAssertEqual(model.firstRunImportError, error)
        XCTAssertFalse(model.firstRunImportBusy)
        try fixture.assertPersisted()
    }

    func testPublicationWarningRemainsVisibleAndPreservesFiles() async throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let model = LibraryModel(rootURL: fixture.library, migrateLegacy: false)
        let error = await model.importExistingHvfVM(fixture.inputs(),
            snapshotHelper: HvfMediaImportTestSupport.helper, worker: { inputs, root, helper, _ in
                FirstRunImportWorker.run(inputs, libraryRoot: root, snapshotHelper: helper, publish: { config, root in
                    VMLibrary.saveOutcome(config, rootURL: root) { data, url in
                        VMRegistrationWriter.commit(data, to: url, syncParent: { _ in throw CocoaError(.fileWriteUnknown) })
                    }
                })
            }, adopt: { _ in XCTFail("uncertain save must remain visible in the import screen"); return true })
        XCTAssertNotNil(error)
        XCTAssertEqual(model.firstRunImportError, error)
        XCTAssertTrue(model.vms.isEmpty)
        XCTAssertFalse(model.firstRunImportBusy)
        try fixture.assertPersisted()
    }

    func testFailedCopyClearsBusyStateAndAllowsActualRetry() async throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let model = LibraryModel(rootURL: fixture.library, migrateLegacy: false)
        let failure = await model.importExistingHvfVM(fixture.inputs(),
            snapshotHelper: fixture.root.appendingPathComponent("missing-helper"))
        XCTAssertNotNil(failure)
        XCTAssertEqual(model.firstRunImportError, failure)
        XCTAssertFalse(model.firstRunImportBusy)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path))
        let retry = await model.importExistingHvfVM(fixture.inputs(), snapshotHelper: HvfMediaImportTestSupport.helper)
        XCTAssertNil(retry)
        XCTAssertNil(model.firstRunImportError)
        XCTAssertFalse(model.firstRunImportBusy)
        try fixture.assertPersisted()
    }
}
