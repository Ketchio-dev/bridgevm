import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryModelFirstRunImportTests: XCTestCase {
    private final class WorkerGate: @unchecked Sendable {
        let started = DispatchSemaphore(value: 0)
        let proceed = DispatchSemaphore(value: 0)
        private let lock = NSLock()
        private var count = 0
        var calls: Int { lock.lock(); defer { lock.unlock() }; return count }

        func enter() -> Bool {
            lock.lock(); count += 1; lock.unlock()
            let isWorker = !Thread.isMainThread
            started.signal()
            // A regression to the main thread fails immediately instead of deadlocking XCTest.
            return isWorker && proceed.wait(timeout: .now() + 10) == .success
        }
    }

    func testWorkerKeepsMainActorResponsiveRefusesDuplicateAndCompletesAfterCancellation() async throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let model = LibraryModel(rootURL: fixture.library, migrateLegacy: false)
        let gate = WorkerGate()
        let task = Task {
            await model.importExistingHvfVM(fixture.inputs(), snapshotHelper: HvfMediaImportTestSupport.helper,
                worker: { inputs, root, helper, _ in
                    guard gate.enter() else { return .failed("worker did not run off main thread") }
                    return FirstRunImportWorker.run(inputs, libraryRoot: root, snapshotHelper: helper)
                })
        }
        let began = await Task.detached { gate.started.wait(timeout: .now() + 5) == .success }.value
        XCTAssertTrue(began)
        XCTAssertTrue(model.firstRunImportBusy)
        let duplicate = await model.importExistingHvfVM(fixture.inputs(), worker: { _, _, _, _ in
            XCTFail("busy model must not start another worker")
            return .failed("duplicate")
        })
        XCTAssertNotNil(duplicate)
        XCTAssertEqual(gate.calls, 1)
        task.cancel()
        gate.proceed.signal()
        let error = await task.value
        XCTAssertNil(error)
        XCTAssertFalse(model.firstRunImportBusy)
        XCTAssertNil(model.firstRunImportError)
        XCTAssertEqual(model.selectedID, "imported")
        try fixture.assertPersisted()
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
                FirstRunImportWorker.run(inputs, libraryRoot: root, snapshotHelper: helper) { config, root in
                    VMLibrary.saveOutcome(config, rootURL: root) { data, url in
                        VMRegistrationWriter.commit(data, to: url, syncParent: { _ in throw CocoaError(.fileWriteUnknown) })
                    }
                }
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
