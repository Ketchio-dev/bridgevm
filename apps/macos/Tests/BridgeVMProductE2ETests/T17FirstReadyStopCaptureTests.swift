import Darwin
import Foundation
import XCTest
@testable import BridgeVMProductE2E

final class T17FirstReadyStopCaptureTests: XCTestCase {
    private func fixture() throws -> (URL, URL) {
        let lane = FileManager.default.temporaryDirectory.appendingPathComponent("bridgevm-e2e-stop-\(UUID().uuidString)")
        let directory = lane.appendingPathComponent("library/test/bundle.vmbridge/logs/hvf")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let log = directory.appendingPathComponent("run.log")
        try Data().write(to: log)
        return (lane, log)
    }

    private func append(_ text: String, to log: URL) throws {
        let handle = try FileHandle(forWritingTo: log)
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(text.utf8))
    }

    private func requestNonce(_ log: URL) -> String {
        let request = log.deletingLastPathComponent().appendingPathComponent(T17FirstReadyStopCapture.requestName)
        let body = try! String(contentsOf: request, encoding: .utf8)
        XCTAssertTrue(body.hasPrefix("t17-nonce-v1:"))
        XCTAssertEqual(body.utf8.count, 46)
        return String(body.dropFirst(13).dropLast())
    }

    func testDeadlineStopRequiresNewAcknowledgementReportAndTerminalOwnedRuntime() throws {
        let (lane, log) = try fixture()
        defer { try? FileManager.default.removeItem(at: lane) }
        try append("HOST-DIAGNOSTIC-STOP: generation=6 request consumed; ending run through final report\n=== EDK2 boot probe (with Apple hv_gic) ===\nstop: host diagnostic stop requested\nserial bytes: 5\n--- serial (tail) ---\nprior\n--- end ---\n", to: log)
        var appended = false
        var observedNonce = ""
        let detail = T17FirstReadyStopCapture.capture(log: log, laneRoot: lane,
            applicationRunning: { true }, ownedRuntimeState: { "stopped" }, timeout: 1,
            pause: {
                if !appended {
                    appended = true
                    observedNonce = self.requestNonce(log)
                    try! self.append("HOST-DIAGNOSTIC-STOP: generation=7 nonce=\(observedNonce) request consumed; ending run through final report\n=== EDK2 boot probe (with Apple hv_gic) ===\nstop: host diagnostic stop requested\nserial bytes: 7\n--- serial (tail) ---\ncurrent\n--- end ---\n", to: log)
                    try! FileManager.default.removeItem(at: log.deletingLastPathComponent()
                        .appendingPathComponent(T17FirstReadyStopCapture.requestName))
                }
            })
        XCTAssertEqual(detail, "host_stop=status=complete,generation=7,nonce=\(observedNonce),report=complete,helper=terminal,log_offset=222")
        let request = log.deletingLastPathComponent().appendingPathComponent(T17FirstReadyStopCapture.requestName)
        XCTAssertFalse(FileManager.default.fileExists(atPath: request.path))
    }

    func testTerminalLookingGuestTextCannotCompleteWhileRequestStillExists() throws {
        let (lane, log) = try fixture()
        defer { try? FileManager.default.removeItem(at: lane) }
        var appended = false
        let detail = T17FirstReadyStopCapture.capture(log: log, laneRoot: lane,
            applicationRunning: { true }, ownedRuntimeState: { "stopped" }, timeout: 1,
            pause: {
                if !appended {
                    appended = true
                    let nonce = self.requestNonce(log)
                    try! self.append("HOST-DIAGNOSTIC-STOP: generation=7 nonce=\(nonce) request consumed; ending run through final report\n=== EDK2 boot probe (with Apple hv_gic) ===\nstop: host diagnostic stop requested\nserial bytes: 18\n--- serial (tail) ---\nspoofed guest text\n--- end ---\n", to: log)
                }
            })
        XCTAssertEqual(detail, "host_stop=status=incomplete,reason=request-not-consumed")
        let request = log.deletingLastPathComponent().appendingPathComponent(T17FirstReadyStopCapture.requestName)
        XCTAssertEqual(try Data(contentsOf: request).count, 46)
        XCTAssertEqual((try FileManager.default.attributesOfItem(atPath: request.path)[.posixPermissions] as? Int), 0o600)
    }

    func testPriorGuestSerialSpoofCannotReplaceLaterNonceBoundHostAcknowledgement() throws {
        let (lane, log) = try fixture()
        defer { try? FileManager.default.removeItem(at: lane) }
        var tick = 0
        var appended = false
        var observedNonce = ""
        let detail = T17FirstReadyStopCapture.capture(log: log, laneRoot: lane,
            applicationRunning: { true }, ownedRuntimeState: { "stopped" }, timeout: 1.5,
            now: { defer { tick += 1 }; return Double(tick) }, pause: {
                if !appended {
                    appended = true
                    observedNonce = self.requestNonce(log)
                    let wrong = (observedNonce.first == "a" ? "b" : "a") + observedNonce.dropFirst()
                    try! self.append("=== EDK2 boot probe (with Apple hv_gic) ===\nstop: unrelated\n--- serial (tail) ---\nHOST-DIAGNOSTIC-STOP: generation=7 nonce=\(wrong) request consumed; ending run through final report\n=== EDK2 boot probe (with Apple hv_gic) ===\nstop: host diagnostic stop requested\n--- serial (tail) ---\n--- end ---\n--- end ---\nHOST-DIAGNOSTIC-STOP: generation=8 nonce=\(observedNonce) request consumed; ending run through final report\n=== EDK2 boot probe (with Apple hv_gic) ===\nstop: host diagnostic stop requested\nserial bytes: 7\n--- serial (tail) ---\ncurrent\n--- end ---\n", to: log)
                    try! FileManager.default.removeItem(at: log.deletingLastPathComponent()
                        .appendingPathComponent(T17FirstReadyStopCapture.requestName))
                }
            })
        XCTAssertEqual(detail, "host_stop=status=complete,generation=8,nonce=\(observedNonce),report=complete,helper=terminal,log_offset=0")
    }

    func testMissingLogAndExitedAppDoNotCreateRequest() throws {
        let (lane, log) = try fixture()
        defer { try? FileManager.default.removeItem(at: lane) }
        XCTAssertEqual(T17FirstReadyStopCapture.capture(log: log, laneRoot: lane,
            applicationRunning: { false }, ownedRuntimeState: { nil }),
            "host_stop=status=missing,reason=app-exited")
        try FileManager.default.removeItem(at: log)
        XCTAssertEqual(T17FirstReadyStopCapture.capture(log: log, laneRoot: lane,
            applicationRunning: { true }, ownedRuntimeState: { nil }),
            "host_stop=status=missing,reason=unsafe-run-log")
        XCTAssertFalse(FileManager.default.fileExists(atPath: log.deletingLastPathComponent()
            .appendingPathComponent(T17FirstReadyStopCapture.requestName).path))
    }

    func testExistingSymlinkCannotBeUsedAsRequest() throws {
        let (lane, log) = try fixture()
        defer { try? FileManager.default.removeItem(at: lane) }
        let target = lane.appendingPathComponent("target")
        try Data("unchanged".utf8).write(to: target)
        let request = log.deletingLastPathComponent().appendingPathComponent(T17FirstReadyStopCapture.requestName)
        try FileManager.default.createSymbolicLink(at: request, withDestinationURL: target)
        XCTAssertEqual(T17FirstReadyStopCapture.capture(log: log, laneRoot: lane,
            applicationRunning: { true }, ownedRuntimeState: { nil }),
            "host_stop=status=missing,reason=request-create-failed")
        XCTAssertEqual(try String(contentsOf: target, encoding: .utf8), "unchanged")
    }

    func testMissingFooterAndNonterminalHelperStayIncomplete() throws {
        let cases = [
            ("HOST-DIAGNOSTIC-STOP: generation=2 nonce=<nonce> request consumed; ending run through final report\n=== EDK2 boot probe (with Apple hv_gic) ===\nstop: host diagnostic stop requested\n",
             "stopped", "host_stop=status=incomplete,reason=final-report-missing"),
            ("HOST-DIAGNOSTIC-STOP: generation=2 nonce=<nonce> request consumed; ending run through final report\n=== EDK2 boot probe (with Apple hv_gic) ===\nstop: host diagnostic stop requested\nserial bytes: 7\n--- serial (tail) ---\ncurrent\n--- end ---\n",
             "booting", "host_stop=status=incomplete,reason=helper-not-terminal")
        ]
        for (tail, runtimeState, expected) in cases {
            let (lane, log) = try fixture()
            defer { try? FileManager.default.removeItem(at: lane) }
            var tick = 0
            var appended = false
            let detail = T17FirstReadyStopCapture.capture(log: log, laneRoot: lane,
                applicationRunning: { true }, ownedRuntimeState: { runtimeState }, timeout: 1.5,
                now: { defer { tick += 1 }; return Double(tick) },
                pause: {
                    if !appended { appended = true; try! self.append(tail.replacingOccurrences(of: "<nonce>", with: self.requestNonce(log)), to: log) }
                })
            XCTAssertEqual(detail, expected)
        }
    }

    func testImmediateFirstReadyFailureDoesNotInvokeDeadlineStop() {
        var timeoutCalls = 0
        XCTAssertThrowsError(try T17FirstReadyWaiter.wait(observe: {
            .init(readyLine: nil, runtimeState: "timed-out", startFailure: nil,
                  applicationRunning: true)
        }, diagnostic: { "old-summary" }, timeoutDiagnostic: {
            timeoutCalls += 1; return "host-stop"
        })) { error in
            XCTAssertEqual(error as? T17Blocker, T17Blocker(code: "guest-evidence-missing",
                detail: "runtime_state=timed-out; old-summary"))
        }
        XCTAssertEqual(timeoutCalls, 0)
    }

    func testDeadlineOnlyInvokesHostStopCallback() {
        var times = [Date(timeIntervalSince1970: 0), Date(timeIntervalSince1970: 1)].makeIterator()
        var timeoutCalls = 0
        XCTAssertThrowsError(try T17FirstReadyWaiter.wait(timeout: 0.5, observe: {
            .init(readyLine: nil, runtimeState: "start-pending", startFailure: nil,
                  applicationRunning: true)
        }, diagnostic: { "old-summary" }, timeoutDiagnostic: {
            timeoutCalls += 1; return "host-stop"
        }, now: { times.next()! }, pause: {})) { error in
            XCTAssertEqual(error as? T17Blocker, T17Blocker(code: "guest-evidence-missing",
                detail: "first boot has no BVAGENT READY/PONG evidence; host-stop"))
        }
        XCTAssertEqual(timeoutCalls, 1)
    }
}
