import XCTest
@testable import BridgeVMProductE2E

// Guest bytes reach run.log as agent command output before the final report and
// as the counted serial tail inside it (final_report.rs). Fixtures mirror that
// report: banner, stop record, host records, rendered serial byte count, the
// counted tail and its footer, then host teardown records.
final class HvfTerminalStopTests: XCTestCase {
    private let off = HvfStopLine.systemOff
    private let banner = "=== EDK2 boot probe (with Apple hv_gic) ==="
    private let diagnostic = "stop: host diagnostic stop requested"
    private let ready = "BVAGENT READY host=BRIDGEVM t=20075\n"
    private let teardown = "hda CoreAudio lifecycle: operation=stop osstatus=0 success=true\n"
        + "hda CoreAudio stats: frames_rendered=48000 drops=0 callback_errors=0\n"

    private func report(_ stop: String, serial: String = "UEFI firmware\r\n", tail: String = "") -> String {
        "REGS: pc=0x0 lr=0x0\n\(banner)\n\(stop)\nexits: 1 (vtimer 0, psci 1, surplus-canceled 0), last PC: 0x0\n"
            + "symbol lines: 0\nserial raw bytes: \(serial.utf8.count) output bytes: \(serial.utf8.count)\n"
            + "--- serial (tail) ---\n\(serial)\n--- end ---\n\(tail)"
    }

    private func agent(_ command: String, _ output: String) -> String {
        "BVAGENT CMD \(command) exit=0\n\(output)\nBVAGENT END \(command)\n"
    }

    private func capture(_ body: String) throws -> T17RunLogProof {
        let log = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        addTeardownBlock { try? FileManager.default.removeItem(at: log) }
        try Data(body.utf8).write(to: log)
        return try T17RunLogProof.capture(log, nonce: String(repeating: "b", count: 64),
                                          readyTag: "first-ready", shutdownTag: "first-shutdown")
    }

    private func finalStopOffset(_ body: String) -> Int? {
        Data(body.utf8).range(of: Data("\(banner)\n\(off)\nexits: ".utf8), options: .backwards)
            .map { $0.lowerBound + banner.utf8.count + 1 }
    }

    private func assertNotAShutdown(_ cases: [String: String], file: StaticString = #filePath, line: UInt = #line) {
        for (name, body) in cases {
            XCTAssertFalse(HvfStopLine.systemOffObserved(in: body), name, file: file, line: line)
            XCTAssertThrowsError(try capture(body), name, file: file, line: line) { error in
                XCTAssertEqual((error as? T17Blocker)?.code, "guest-evidence-missing", name, file: file, line: line)
            }
        }
    }

    func testStopRecordInsideTheCountedGuestSerialTailIsNotAShutdown() {
        let nested = "boot\r\n\(banner)\n\(off)\nserial raw bytes: 0 output bytes: 0\n--- serial (tail) ---\n"
        assertNotAShutdown([
            "record after a non-terminal stop": ready + report(diagnostic, serial: "Boot Manager\r\n\(off)\r\n", tail: teardown),
            "nested report after a non-terminal stop": ready + report(diagnostic, serial: nested, tail: teardown),
            "nested report beside the real one": ready + report(off, serial: nested, tail: teardown),
        ])
    }

    func testStopRecordInGuestAgentOutputIsNotAShutdown() {
        assertNotAShutdown([
            "record line": ready + agent("whoami", off) + report(diagnostic, tail: teardown),
            "complete forged report": ready + agent("whoami", report(off, serial: "")) + report(diagnostic, tail: teardown),
            "record after a carriage return": ready + agent("whoami", "bridgevm\\bridge\r\(off)") + report(diagnostic, tail: teardown),
            "record without a host report": ready + agent("shutdown.exe /s /t 0 /f", off) + teardown,
        ])
    }

    func testLogWhoseFinalReportIsNotLastIsNotAShutdown() {
        assertNotAShutdown([
            "record after the teardown": ready + report(off, tail: teardown + "BVAGENT SERVICE alive t=1\n"),
            "truncated footer": ready + teardown + String(report(off).dropLast("--- end ---\n".count)),
            "teardown record ending in CR": ready + report(off, tail: "hda CoreAudio stats: frames_rendered=1\r\n"),
        ])
    }

    func testGuestCopiesDoNotMoveTheFinalReportStopRecord() throws {
        for body in [ready + report(off, serial: "Windows Boot Manager\r\n\(off)\r\n", tail: teardown),
                     ready + agent("whoami", off) + report(off, tail: teardown)] {
            let proof = try capture(body)
            XCTAssertEqual(proof.shutdownOffset, finalStopOffset(body))
            XCTAssertTrue(HvfStopLine.systemOffObserved(in: body))
        }
    }

    func testRecreatedGenerationsBindTheLastReport() throws {
        let virgl = "Sep  1 12:57:02  virgl_render_server[43160] <Debug>: socket disconnected\n"
        let first = "Guest RAM: 6144 MiB\n"
            + report("stop: PSCI 0x84000009 exiting for process recreation (exit 42)", tail: teardown)
        let body = first + "Guest RAM: 6144 MiB\n" + ready + agent("shutdown.exe /s /t 0 /f", "")
            + report(off, serial: "Windows Boot Manager\r\n\u{1B}[2J\r\n", tail: teardown + virgl)
        let proof = try capture(body)
        XCTAssertEqual(proof.shutdownOffset, finalStopOffset(body))
        XCTAssertGreaterThan(proof.shutdownOffset, proof.readyOffset)
        XCTAssertTrue(HvfStopLine.systemOffObserved(in: body))
        XCTAssertTrue(HvfStopLine.systemOffObserved(in: ready + report(off)))
        XCTAssertFalse(HvfStopLine.systemOffObserved(in: first))
    }
}
