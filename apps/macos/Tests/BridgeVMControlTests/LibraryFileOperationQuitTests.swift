import XCTest
@testable import BridgeVMControl

/// Quitting during a Move, Duplicate or Delete waits for it. A quit used to exit at once and cut a
/// move's copy short, leaving the VM hidden behind its unfinished-move record at the next launch.
@MainActor
final class LibraryFileOperationQuitTests: XCTestCase {
    func testQuitDuringAMoveWaitsForTheMoveThenQuits() async throws {
        let f = try LibraryMoveSelectionFixture()
        try f.admit()
        var confirmations = 0, retries = 0
        let termination = ControlAppTermination(sessions: { f.library.appTerminationSessions() },
            presenter: .init(confirmQuit: { _ in XCTFail("No VM is running"); return false },
                decideAtDeadline: { _, _ in .cancelQuit },
                confirmAfterPreparation: { confirmations += 1; return true }),
            reply: { _ in XCTFail("Unexpected modal quit") }, preparationLibrary: { f.library },
            retryQuit: { retries += 1 })

        XCTAssertEqual(termination.shouldTerminate(reason: .user), .terminateCancel)
        XCTAssertEqual(confirmations, 1)
        XCTAssertEqual(f.library.quitAfterVMPreparation?.confirmed, true)

        try await f.finish()
        try f.assertMoved()
        await withCheckedContinuation { continuation in
            DispatchQueue.main.async { continuation.resume() }
        }
        XCTAssertEqual(retries, 1, "the quit goes ahead once the move is done")
        XCTAssertNil(f.library.quitAfterVMPreparation)
        XCTAssertEqual(termination.shouldTerminate(reason: .user), .terminateNow)
    }
}
