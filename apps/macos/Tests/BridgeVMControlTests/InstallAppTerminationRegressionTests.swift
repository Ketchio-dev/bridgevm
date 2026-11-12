import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class InstallAppTerminationRegressionTests: XCTestCase {
    func testQuitIncludesInstallAndWaitsForCancellationCleanupOrCommit() async throws {
        for committing in [false, true] {
            let f = HvfWindowsInstallStoreFixture(); defer { f.clean() }
            let config = f.config(); try f.save(config)
            let library = f.library(), session = library.windowsInstallSession(for: config)
            await (try XCTUnwrap(session.start())).value
            session.commitStarted = committing
            var questions = 0, deadlines = 0, replies: [Bool] = [], clock: TimeInterval = 100
            let termination = ControlAppTermination(sessions: { library.appTerminationSessions() },
                presenter: .init(confirmQuit: { _ in questions += 1; return true },
                    decideAtDeadline: { _, _ in deadlines += 1; return .quitAnyway }),
                reply: { replies.append($0) }, now: { clock })
            XCTAssertEqual(termination.shouldTerminate(reason: .user), .terminateLater)
            XCTAssertEqual(questions, 1)
            XCTAssertEqual(session.cancelled, !committing)
            XCTAssertTrue(session.isRunning)
            clock += ControlAppTermination.deadline * 3
            termination.poll()
            XCTAssertEqual(deadlines, committing ? 0 : 1)
            XCTAssertEqual(replies, committing ? [] : [true], "Only publication prevents Quit Anyway")
            if committing { session.finish(.done) }
            else { await f.finishCancelled(session) }
            termination.poll()
            XCTAssertEqual(replies, [true])
        }
    }

    func testSystemQuitCancelsInstallWithoutQuestion() async throws {
        let f = HvfWindowsInstallStoreFixture(); defer { f.clean() }
        let config = f.config(); try f.save(config)
        let library = f.library(), session = library.windowsInstallSession(for: config)
        await (try XCTUnwrap(session.start())).value
        var replies: [Bool] = []
        let termination = ControlAppTermination(sessions: { library.appTerminationSessions() },
            presenter: .init(confirmQuit: { _ in XCTFail("System quit must not ask"); return false },
                decideAtDeadline: { _, _ in .keepWaiting }), reply: { replies.append($0) })
        XCTAssertEqual(termination.shouldTerminate(reason: .logoutRestartOrShutdown), .terminateLater)
        XCTAssertTrue(session.cancelled)
        XCTAssertEqual(replies, [])
        await f.finishCancelled(session)
        termination.poll()
        XCTAssertEqual(replies, [true])
    }
}
