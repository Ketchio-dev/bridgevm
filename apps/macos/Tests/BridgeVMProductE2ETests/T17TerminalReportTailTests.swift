import Foundation
import XCTest
@testable import BridgeVMProductE2E

final class T17TerminalReportTailTests: XCTestCase {
    private let audioTail = """
    hda CoreAudio callback enqueue: state=stopping reason=stopping-enqueue-during-reset osstatus=-66632 expected=true
    hda CoreAudio callback enqueue: state=stopping reason=stopping-enqueue-during-reset osstatus=-66632 expected=true
    hda CoreAudio callback enqueue: state=stopping reason=stopping-enqueue-during-reset osstatus=-66632 expected=true
    hda CoreAudio lifecycle: operation=stop osstatus=0 success=true
    hda CoreAudio lifecycle: operation=dispose osstatus=0 success=true
    hda CoreAudio stats: frames_rendered=173871 drops=0 callback_errors=3

    """

    private func capture(with tail: String, serial: String = "firmware",
                         hostFooter: Bool = true, serialByteCount: Int? = nil,
                         outputByteCount: Int? = nil) throws -> String {
        let lane = FileManager.default.temporaryDirectory.appendingPathComponent("bridgevm-e2e-stop-tail-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: lane) }
        let evidence = lane.appendingPathComponent("library/test/bundle.vmbridge/logs/hvf")
        try FileManager.default.createDirectory(at: evidence, withIntermediateDirectories: true)
        let log = evidence.appendingPathComponent("run.log")
        try Data().write(to: log)
        var tick = 0
        var appended = false
        return T17FirstReadyStopCapture.capture(log: log, laneRoot: lane,
            applicationRunning: { true }, ownedRuntimeState: { "stopped" }, timeout: 1.5,
            now: { defer { tick += 1 }; return Double(tick) }, pause: {
                if appended { return }
                appended = true
                let request = evidence.appendingPathComponent(T17FirstReadyStopCapture.requestName)
                let body = try! String(contentsOf: request, encoding: .utf8)
                let nonce = String(body.dropFirst("t17-nonce-v1:".count).dropLast())
                let rawCount = serialByteCount ?? serial.utf8.count
                let countLine = outputByteCount.map { "serial raw bytes: \(rawCount) output bytes: \($0)" }
                    ?? "serial bytes: \(rawCount)"
                let report = "HOST-DIAGNOSTIC-STOP: generation=2 nonce=\(nonce) request consumed; ending run through final report\n"
                    + "=== EDK2 boot probe (with Apple hv_gic) ===\nstop: host diagnostic stop requested\n"
                    + "\(countLine)\n--- serial (tail) ---\n" + serial
                    + (hostFooter ? "\n--- end ---\n" : "") + tail
                let handle = try! FileHandle(forWritingTo: log)
                try! handle.seekToEnd()
                try! handle.write(contentsOf: Data(report.utf8))
                try! handle.close()
                try! FileManager.default.removeItem(at: request)
            })
    }

    func testCompletedReportWithRealShapeCoreAudioShutdownTail() throws {
        XCTAssertTrue(try capture(with: audioTail).hasPrefix(
            "host_stop=status=complete,generation=2,"))
    }

    func testArbitraryPostFooterTextStaysIncomplete() throws {
        XCTAssertEqual(try capture(with: "guest-forged-trailer\n"),
                       "host_stop=status=incomplete,reason=final-report-missing")
    }

    func testTruncatedOrOversizedHostTailStaysIncomplete() throws {
        XCTAssertEqual(try capture(with: "hda CoreAudio lifecycle: operation=stop osstatus=0 success=true"),
                       "host_stop=status=incomplete,reason=final-report-missing")
        XCTAssertEqual(try capture(with: String(repeating: audioTail, count: 9)),
                       "host_stop=status=incomplete,reason=final-report-missing")
    }

    func testGuestSerialFooterCannotReplaceMissingHostFooter() throws {
        XCTAssertEqual(try capture(with: "", serial: "firmware\n--- end ---\n", hostFooter: false),
                       "host_stop=status=incomplete,reason=final-report-missing")
    }

    func testLossyUTF8SerialLengthCannotShiftTheFooter() throws {
        XCTAssertEqual(try capture(with: "", serial: "\u{FFFD}", serialByteCount: 1),
                       "host_stop=status=incomplete,reason=final-report-missing")
    }

    func testNewFormatCountsRenderedSerialPastGuestFooter() throws {
        let serial = String(repeating: "\u{FFFD}", count: 7) + "\n--- end ---\nX"
        XCTAssertTrue(try capture(with: audioTail, serial: serial, serialByteCount: 21,
                                  outputByteCount: serial.utf8.count).hasPrefix(
            "host_stop=status=complete,generation=2,"))
    }

    func testPartialLossyGuestFooterRejectsLegacyAndNewFormat() throws {
        for padding in ["", "AAAAA"] {
            let rawCount = 21 + padding.utf8.count
            let partial = String(repeating: "\u{FFFD}", count: 7) + padding + "\n--- end ---\n"
            XCTAssertEqual(try capture(with: "", serial: partial, hostFooter: false,
                                       serialByteCount: rawCount),
                           "host_stop=status=incomplete,reason=final-report-missing")
            XCTAssertEqual(try capture(with: "", serial: partial, hostFooter: false,
                                       serialByteCount: rawCount, outputByteCount: rawCount + 14),
                           "host_stop=status=incomplete,reason=final-report-missing")
        }
    }

    func testEarlierSameNonceForgedReportFailsClosed() {
        let nonce = String(repeating: "a", count: 32)
        let ack = "HOST-DIAGNOSTIC-STOP: generation=2 nonce=\(nonce) request consumed; ending run through final report\n"
        let body = "=== EDK2 boot probe (with Apple hv_gic) ===\nstop: host diagnostic stop requested\n"
        let fake = ack + body + "serial bytes: 4\n--- serial (tail) ---\nfake\n--- end ---\n"
        let genuine = ack + body + "serial bytes: 4\n--- serial (tail) ---\nreal\n--- end ---\n"
        XCTAssertFalse(T17TerminalReportTail.isComplete(rawSuffix: Data((fake + genuine).utf8),
                       nonce: nonce, generation: 2))
    }
}
