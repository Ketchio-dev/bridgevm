import XCTest
@testable import BridgeVMControl

@MainActor
final class FirstRunImportRecoveryTests: XCTestCase {
    private func uncertainImport(_ model: LibraryModel, fixture: FirstRunImportTransactionFixture) async -> String? {
        await model.importExistingHvfVM(fixture.inputs(), snapshotHelper: HvfMediaImportTestSupport.helper,
            worker: { inputs, root, helper, progress in
                FirstRunImportWorker.run(inputs, libraryRoot: root, snapshotHelper: helper, progress: progress) { config, root in
                    VMLibrary.saveOutcome(config, rootURL: root) { data, url in
                        VMRegistrationWriter.commit(data, to: url, syncParent: { _ in throw CocoaError(.fileWriteUnknown) })
                    }
                }
            })
    }

    func testReadBackSelectsPublishedVMWithoutOriginalSourceFiles() async throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let model = LibraryModel(rootURL: fixture.library, migrateLegacy: false)
        model.proMode = true
        model.selectedID = LibraryModel.firstRunImportSelectionID
        let warning = await uncertainImport(model, fixture: fixture)
        XCTAssertNotNil(warning)
        XCTAssertEqual(model.firstRunImport.publishedConfig?.slug, "imported")
        XCTAssertEqual(model.selectedID, LibraryModel.firstRunImportSelectionID)
        XCTAssertTrue(model.proMode)
        let configURL = fixture.entry.appendingPathComponent("vm.json")
        let originalConfig = try Data(contentsOf: configURL)
        try FileManager.default.removeItem(at: fixture.disk)
        try FileManager.default.removeItem(at: fixture.vars)

        let error = await model.recoverPublishedFirstRunImport()

        XCTAssertNil(error)
        XCTAssertNil(model.firstRunImportError)
        XCTAssertNil(model.firstRunImport.publishedConfig)
        XCTAssertFalse(model.firstRunImportBusy)
        XCTAssertEqual(model.selectedID, "imported")
        XCTAssertFalse(model.proMode)
        XCTAssertEqual(try Data(contentsOf: configURL), originalConfig)
        XCTAssertEqual(try Data(contentsOf: fixture.entry.appendingPathComponent("bundle/disks/hvf-target.raw")),
            Data([1, 2, 3, 4]))
    }

    func testAdoptionWarningCanRecoverExistingRegistration() async throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let model = LibraryModel(rootURL: fixture.library, migrateLegacy: false)
        let warning = await model.importExistingHvfVM(fixture.inputs(),
            snapshotHelper: HvfMediaImportTestSupport.helper, adopt: { _ in false })
        XCTAssertNotNil(warning)
        XCTAssertEqual(model.firstRunImport.publishedConfig?.slug, "imported")
        let recovered = await model.recoverPublishedFirstRunImport()
        XCTAssertNil(recovered)
        XCTAssertEqual(model.selectedID, "imported")
        try fixture.assertPersisted()
    }

    func testReadBackFailureRetainsWarningAndAllowsRetryWithoutCopy() async throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let model = LibraryModel(rootURL: fixture.library, migrateLegacy: false)
        let result = await uncertainImport(model, fixture: fixture)
        let warning = try XCTUnwrap(result)
        let configURL = fixture.entry.appendingPathComponent("vm.json")
        let originalConfig = try Data(contentsOf: configURL)
        try Data("incomplete registration".utf8).write(to: configURL)

        let failure = await model.recoverPublishedFirstRunImport()

        XCTAssertTrue(try XCTUnwrap(failure).contains(warning))
        XCTAssertTrue(try XCTUnwrap(failure).contains("저장된 VM 불러오기"))
        XCTAssertEqual(model.firstRunImport.publishedConfig?.slug, "imported")
        XCTAssertFalse(model.firstRunImportBusy)
        XCTAssertEqual(try Data(contentsOf: fixture.entry.appendingPathComponent("bundle/disks/hvf-target.raw")),
            Data([1, 2, 3, 4]))
        try originalConfig.write(to: configURL)
        let retried = await model.recoverPublishedFirstRunImport()
        XCTAssertNil(retried)
        XCTAssertNil(model.firstRunImportError)
        try fixture.assertPersisted()
    }

    func testPublishedWarningBlocksRepeatImportUntilReadBack() async throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let model = LibraryModel(rootURL: fixture.library, migrateLegacy: false)
        let warning = await uncertainImport(model, fixture: fixture)
        let retry = await model.importExistingHvfVM(fixture.inputs(), worker: { _, _, _, _ in
            XCTFail("already published import must not copy again")
            return .failed("unexpected copy")
        })
        XCTAssertEqual(retry, warning)
        XCTAssertEqual(model.firstRunImportError, warning)
        XCTAssertEqual(model.firstRunImport.publishedConfig?.slug, "imported")
        XCTAssertFalse(model.firstRunImportBusy)
        try fixture.assertPersisted()
    }

    func testExplicitReturnAfterFailedReadBackAllowsDistinctImportAndPreservesEarlierFiles() async throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let model = LibraryModel(rootURL: fixture.library, migrateLegacy: false)
        let warning = await uncertainImport(model, fixture: fixture)
        XCTAssertNotNil(warning)
        let configURL = fixture.entry.appendingPathComponent("vm.json")
        let brokenConfig = Data("incomplete registration".utf8)
        try brokenConfig.write(to: configURL)
        let failed = await model.recoverPublishedFirstRunImport()
        XCTAssertNotNil(failed)

        XCTAssertTrue(model.returnToFirstRunInputs())
        XCTAssertNil(model.firstRunImport.publishedConfig)
        XCTAssertNil(model.firstRunImportError)
        let imported = await model.importExistingHvfVM(fixture.inputs("second"),
            snapshotHelper: HvfMediaImportTestSupport.helper)

        XCTAssertNil(imported)
        XCTAssertEqual(model.selectedID, "second")
        XCTAssertEqual(try Data(contentsOf: configURL), brokenConfig)
        XCTAssertEqual(try Data(contentsOf: fixture.entry.appendingPathComponent("bundle/disks/hvf-target.raw")),
            Data([1, 2, 3, 4]))
        XCTAssertEqual(try Data(contentsOf: fixture.entry.appendingPathComponent("bundle/metadata/hvf-vars.fd")),
            try Data(contentsOf: fixture.vars))
        try fixture.assertSources()
    }

    private final class ReadbackGate: @unchecked Sendable {
        let started = DispatchSemaphore(value: 0)
        let proceed = DispatchSemaphore(value: 0)
        func waitUntilStarted() async -> Bool {
            await withCheckedContinuation { continuation in
                DispatchQueue.global().async {
                    continuation.resume(returning: self.started.wait(timeout: .now() + 5) == .success)
                }
            }
        }
    }

    func testBusyReadBackCannotResetPendingImportAndCompletesAfterCancellation() async throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let model = LibraryModel(rootURL: fixture.library, migrateLegacy: false)
        let warning = await uncertainImport(model, fixture: fixture)
        XCTAssertNotNil(warning)
        let gate = ReadbackGate()
        let task = Task {
            await model.recoverPublishedFirstRunImport { expected, root in
                gate.started.signal()
                guard !Thread.isMainThread, gate.proceed.wait(timeout: .now() + 10) == .success else { return nil }
                return VMLibrary.list(rootURL: root).first { $0.slug == expected.slug && $0.bundlePath == expected.bundlePath }
            }
        }
        let began = await gate.waitUntilStarted()
        XCTAssertTrue(began)
        XCTAssertTrue(model.firstRunImportBusy)
        XCTAssertFalse(model.returnToFirstRunInputs())
        XCTAssertEqual(model.firstRunImportError, warning)
        XCTAssertEqual(model.firstRunImport.publishedConfig?.slug, "imported")
        task.cancel()
        gate.proceed.signal()
        let error = await task.value
        XCTAssertNil(error)
        XCTAssertFalse(model.firstRunImportBusy)
        XCTAssertEqual(model.selectedID, "imported")
        try fixture.assertPersisted()
    }

}
