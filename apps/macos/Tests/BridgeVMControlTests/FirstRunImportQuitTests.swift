import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class FirstRunImportQuitTests: XCTestCase {
    func testAcceptedQuitWaitsForImportToPublish() async throws {
        try await checkQuit()
    }

    func testCancelingQuitStillPublishesTheImport() async throws {
        try await checkQuit(cancel: true)
    }

    func testFailedImportCancelsQuitAndKeepsErrorVisible() async throws {
        try await checkQuit(fail: true)
    }

    private func checkQuit(cancel: Bool = false, fail: Bool = false) async throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let library = LibraryModel(rootURL: fixture.library, migrateLegacy: false)
        let reachedPublication = DispatchSemaphore(value: 0)
        let allowPublication = DispatchSemaphore(value: 0)
        var confirmations = 0, reveals = 0, quitRequests = 0
        let termination = ControlAppTermination(sessions: { library.appTerminationSessions() },
            presenter: .init(confirmQuit: { _ in XCTFail("Unexpected VM stop prompt"); return false },
                decideAtDeadline: { _, _ in .cancelQuit }, confirmAfterImport: { confirmations += 1; return true },
                revealImport: { reveals += 1 }), reply: { _ in XCTFail("Quit must leave the main run loop available") },
            importLibrary: { library }, retryQuit: { quitRequests += 1 })

        let operation = Task {
            await library.importExistingHvfVM(fixture.inputs(), snapshotHelper: HvfMediaImportTestSupport.helper,
                worker: { inputs, root, helper, progress in
                    FirstRunImportWorker.run(inputs, libraryRoot: root, snapshotHelper: helper, progress: progress) { config, root in
                        reachedPublication.signal()
                        guard allowPublication.wait(timeout: .now() + 15) == .success, !fail else {
                            return .notPublished(CocoaError(.fileWriteUnknown))
                        }
                        return VMLibrary.createOutcome(config, rootURL: root)
                    }
                })
        }
        let reached = await waitOffCooperativePool(reachedPublication, seconds: 10)
        XCTAssertTrue(reached)
        XCTAssertEqual(termination.shouldTerminate(reason: .user), .terminateCancel)
        XCTAssertTrue(library.quitAfterFirstRunImport)
        XCTAssertEqual(library.selectedID, LibraryModel.firstRunImportSelectionID)
        XCTAssertEqual(confirmations, 1)
        XCTAssertEqual(reveals, 1)
        XCTAssertEqual(quitRequests, 0)
        if cancel { library.quitAfterFirstRunImport = false }

        allowPublication.signal()
        let error = await operation.value
        await withCheckedContinuation { continuation in
            DispatchQueue.main.async { continuation.resume() }
        }

        XCTAssertFalse(library.quitAfterFirstRunImport)
        XCTAssertEqual(quitRequests, cancel || fail ? 0 : 1)
        if fail {
            XCTAssertNotNil(error)
            XCTAssertEqual(library.firstRunImportError, error)
            XCTAssertEqual(library.selectedID, LibraryModel.firstRunImportSelectionID)
            XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path))
            try fixture.assertSources()
        } else {
            XCTAssertNil(error)
            XCTAssertEqual(library.selectedID, "imported")
            try fixture.assertPersisted()
        }
    }
}
