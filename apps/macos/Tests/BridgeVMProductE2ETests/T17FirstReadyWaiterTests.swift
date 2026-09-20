import XCTest
@testable import BridgeVMProductE2E

final class T17FirstReadyWaiterTests: XCTestCase {
    func testReadyEvidenceWinsOverConcurrentTerminalSignals() {
        var monitor = T17FirstReadyMonitor()
        let decision = monitor.evaluate(.init(readyLine: "BVAGENT READY nonce=x",
            runtimeState: "timed-out", startFailure: "private failure", applicationRunning: false))
        XCTAssertEqual(decision, .ready("BVAGENT READY nonce=x"))
    }

    func testStoppedIsTerminalOnlyAfterRuntimeBecameActive() {
        var monitor = T17FirstReadyMonitor()
        XCTAssertEqual(monitor.evaluate(.init(readyLine: nil, runtimeState: "stopped",
            startFailure: nil, applicationRunning: true)), .pending)
        XCTAssertEqual(monitor.evaluate(.init(readyLine: nil, runtimeState: "start-pending",
            startFailure: nil, applicationRunning: true)), .pending)
        XCTAssertEqual(monitor.evaluate(.init(readyLine: nil, runtimeState: "stopped",
            startFailure: nil, applicationRunning: true)),
            .failed(code: "guest-evidence-missing", reason: "runtime_state=stopped-after-active"))
    }

    func testConnectedThenStoppingIsTerminal() {
        var monitor = T17FirstReadyMonitor()
        XCTAssertEqual(monitor.evaluate(.init(readyLine: nil, runtimeState: "connected",
            startFailure: nil, applicationRunning: true)), .pending)
        XCTAssertEqual(monitor.evaluate(.init(readyLine: nil, runtimeState: "stopping",
            startFailure: nil, applicationRunning: true)),
            .failed(code: "guest-evidence-missing", reason: "runtime_state=stopping-after-active"))
    }

    func testTimedOutAndAppExitFailWithoutWaitingForDeadline() {
        var timedOut = T17FirstReadyMonitor()
        XCTAssertEqual(timedOut.evaluate(.init(readyLine: nil, runtimeState: "timed-out",
            startFailure: nil, applicationRunning: true)),
            .failed(code: "guest-evidence-missing", reason: "runtime_state=timed-out"))
        var exited = T17FirstReadyMonitor()
        XCTAssertEqual(exited.evaluate(.init(readyLine: nil, runtimeState: nil,
            startFailure: nil, applicationRunning: false)),
            .failed(code: "app-launch-failed", reason: "product app exited during first boot"))
    }

    func testStartFailureIsHashedAndNeverCopiedIntoDetail() {
        var monitor = T17FirstReadyMonitor()
        let decision = monitor.evaluate(.init(readyLine: nil, runtimeState: "stopped",
            startFailure: "secret path /private/source.iso", applicationRunning: true))
        guard case let .failed(code, reason) = decision else { return XCTFail("expected failure") }
        XCTAssertEqual(code, "guest-evidence-missing")
        XCTAssertTrue(reason.contains("ui_failure_bytes=31,captured=31,truncated=0,sha256="))
        XCTAssertFalse(reason.contains("secret")); XCTAssertFalse(reason.contains("source.iso"))
    }

    func testDeadlineRetainsTheBoundedRunLogDiagnostic() {
        var times = [Date(timeIntervalSince1970: 0), Date(timeIntervalSince1970: 1)].makeIterator()
        XCTAssertThrowsError(try T17FirstReadyWaiter.wait(timeout: 0.5, observe: {
            .init(readyLine: nil, runtimeState: "start-pending",
                  startFailure: nil, applicationRunning: true)
        }, diagnostic: { "run_log=status=present,sha256=abc" },
            now: { times.next()! }, pause: {})) { error in
            XCTAssertEqual(error as? T17Blocker, T17Blocker(code: "guest-evidence-missing",
                detail: "first boot has no BVAGENT READY/PONG evidence; run_log=status=present,sha256=abc"))
        }
    }
}
