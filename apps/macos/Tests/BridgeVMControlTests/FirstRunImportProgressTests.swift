import XCTest
@testable import BridgeVMControl

final class FirstRunImportProgressTests: XCTestCase {
    private final class Stages: @unchecked Sendable {
        private let lock = NSLock()
        private var values: [FirstRunImportStage] = []
        func append(_ stage: FirstRunImportStage) { lock.lock(); values.append(stage); lock.unlock() }
        var snapshot: [FirstRunImportStage] { lock.lock(); defer { lock.unlock() }; return values }
    }

    private final class Callback: @unchecked Sendable {
        private let lock = NSLock()
        private var value: (@Sendable (FirstRunImportStage) -> Void)?
        func store(_ value: @escaping @Sendable (FirstRunImportStage) -> Void) {
            lock.lock(); self.value = value; lock.unlock()
        }
        func send(_ stage: FirstRunImportStage) {
            lock.lock(); let callback = value; lock.unlock()
            callback?(stage)
        }
    }

    func testActualWorkerReportsStagesAtCopyAndSaveBoundaries() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let stages = Stages()
        let result = FirstRunImportWorker.run(fixture.inputs(), libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper, progress: { stage in
                stages.append(stage)
                let config = fixture.entry.appendingPathComponent("vm.json")
                XCTAssertFalse(FileManager.default.fileExists(atPath: config.path))
                if stage == .saving {
                    do {
                        let data = try Data(contentsOf: fixture.entry.appendingPathComponent("bundle/disks/hvf-target.raw"))
                        XCTAssertEqual(data, Data([1, 2, 3, 4]))
                    } catch { XCTFail("copied disk must exist before saving: \(error)") }
                } else {
                    XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path))
                }
            })
        guard case .committed = result else { return XCTFail("expected committed import") }
        XCTAssertEqual(stages.snapshot, [.validating, .copying, .saving])
        try fixture.assertPersisted()
    }

    func testValidationFailureDoesNotClaimCopyOrSaveStarted() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let stages = Stages()
        let result = FirstRunImportWorker.run(fixture.inputs(""), libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper, progress: { stages.append($0) })
        guard case .failed = result else { return XCTFail("invalid name must fail") }
        XCTAssertEqual(stages.snapshot, [.validating])
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path))
    }

    func testProgressIsMonotonicAndIgnoresFinishedAndPreviousOperations() {
        var state = FirstRunImportProgress()
        let first = state.begin()
        state.advance(.saving, operation: first)
        state.advance(.copying, operation: first)
        XCTAssertEqual(state.stage, .saving)
        state.finish(error: nil)
        state.advance(.readingLibrary, operation: first)
        XCTAssertEqual(state.stage, .complete)
        XCTAssertFalse(state.isBusy)
        let second = state.begin()
        state.advance(.saving, operation: first)
        XCTAssertEqual(state.stage, .validating)
        state.advance(.copying, operation: second)
        XCTAssertEqual(state.stage, .copying)
    }

    @MainActor
    func testSuccessfulImportLeavesProModeAndIgnoresLateWorkerCallback() async throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let model = LibraryModel(rootURL: fixture.library, migrateLegacy: false)
        model.proMode = true
        model.selectedID = LibraryModel.firstRunImportSelectionID
        let callback = Callback()
        let error = await model.importExistingHvfVM(fixture.inputs(), snapshotHelper: HvfMediaImportTestSupport.helper,
            worker: { inputs, root, helper, progress in
                callback.store(progress)
                return FirstRunImportWorker.run(inputs, libraryRoot: root, snapshotHelper: helper, progress: progress)
            })
        XCTAssertNil(error)
        XCTAssertEqual(model.firstRunImport.stage, .complete)
        callback.send(.copying)
        callback.send(.saving)
        await Task.yield()
        XCTAssertFalse(model.firstRunImportBusy)
        XCTAssertEqual(model.firstRunImport.stage, .complete)
        XCTAssertEqual(model.selectedID, "imported")
        XCTAssertFalse(model.proMode)
    }
}
