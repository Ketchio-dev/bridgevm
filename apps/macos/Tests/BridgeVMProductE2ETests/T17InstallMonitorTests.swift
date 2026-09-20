import XCTest
@testable import BridgeVMProductE2E
final class T17InstallMonitorTests: XCTestCase {
    private func run(
        timeout: TimeInterval = 1, running: @escaping () -> Bool = { true },
        installed: @escaping () -> Bool = { false }, stage: @escaping () -> String?,
        failure: @escaping () -> String? = { nil }
    ) throws {
        var now = 0.0
        try T17InstallMonitor.wait(timeout: timeout, clock: { now }, pause: { now += $0 },
                                   applicationIsRunning: running, installedRuntimeVisible: installed,
                                   stage: stage, failure: failure)
    }
    func testCompletedStageOrRuntimeTransitionReturns() {
        XCTAssertNoThrow(try run(stage: { "완료" }))
        XCTAssertNoThrow(try run(installed: { true }, stage: { nil }))
    }
    func testFailedStageRetainsBoundedMessage() {
        XCTAssertThrowsError(try run(stage: { "실패" }, failure: { " measured failure " })) { error in
            XCTAssertEqual(error as? T17Blocker,
                           T17Blocker(code: "installer-failed", detail: "measured failure"))
        }
        XCTAssertThrowsError(try run(stage: { "실패" }, failure: { String(repeating: "x", count: 600) })) { error in
            XCTAssertEqual((error as? T17Blocker)?.detail.count, 512)
        }
    }

    func testFailedStageWithoutMessageFailsClosed() {
        XCTAssertThrowsError(try run(stage: { "실패" })) { error in
            XCTAssertEqual((error as? T17Blocker)?.detail, "product install reported a failed UI stage")
        }
    }

    func testStoppedApplicationAndTimeoutFail() {
        XCTAssertThrowsError(try run(running: { false }, stage: { nil })) { error in
            XCTAssertEqual((error as? T17Blocker)?.detail, "product app exited during installation")
        }
        XCTAssertThrowsError(try run(timeout: 0, stage: { "설치 중" })) { error in
            XCTAssertEqual((error as? T17Blocker)?.detail, "product install did not reach a terminal UI stage")
        }
    }
}
