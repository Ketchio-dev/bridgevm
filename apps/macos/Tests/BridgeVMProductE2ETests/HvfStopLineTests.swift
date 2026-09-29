import CryptoKit
import XCTest
@testable import BridgeVMProductE2E

// Fixtures use the records final_report.rs prints; no runtime path prints the
// retired "stop: PSCI SYSTEM_OFF" literal.
final class HvfStopLineTests: XCTestCase {
    private let off = "stop: PSCI 0x84000008 (system off)"
    private let nonce = String(repeating: "a", count: 64)

    private func capture(_ body: String) throws -> T17RunLogProof {
        let log = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        addTeardownBlock { try? FileManager.default.removeItem(at: log) }
        try Data(body.utf8).write(to: log)
        return try T17RunLogProof.capture(log, nonce: nonce, readyTag: "first-ready", shutdownTag: "first-shutdown")
    }

    func testRunLogProofBindsTheExactRuntimeSystemOffRecord() throws {
        let head = "BVAGENT READY host=x t=1\r\n=== EDK2 boot probe (with Apple hv_gic) ===\n"
        let proof = try capture(head + off + "\r\n" + off + " extra\nexits: 1\n")
        XCTAssertEqual(proof.shutdownOffset, head.utf8.count)
        let bound = Data("bridgevm-t17-first-shutdown-v1\n\(nonce)\n\(off)\n".utf8)
        XCTAssertEqual(proof.shutdownLineHash, SHA256.hash(data: bound).map { String(format: "%02x", $0) }.joined())
    }

    func testRunLogProofRejectsRetiredAndInexactStopRecords() {
        for stop in ["stop: PSCI SYSTEM_OFF", off + " extra", "guest " + off,
                     "stop: PSCI 0x84000009 exiting for process recreation (exit 42)"] {
            XCTAssertThrowsError(try capture("BVAGENT READY host=x t=1\n\(stop)\n"), stop) { error in
                XCTAssertEqual((error as? T17Blocker)?.code, "guest-evidence-missing")
            }
        }
    }

    func testFirstBootDiagnosticCountsRuntimeStopRecordsNotResetBanners() {
        let data = Data(("PSCI SYSTEM_RESET max reboots: 8\nPSCI SYSTEM_RESET: reboot 1/8\n"
            + "stop: PSCI 0x84000009 exiting for process recreation (exit 42)\nPSCI SYSTEM_RESET max reboots: 8\n"
            + "stop: PSCI SYSTEM_OFF\n\(off) extra\n\(off)\r\n").utf8)
        let detail = T17FirstBootDiagnostic.summarize(data, totalBytes: UInt64(data.count))
        XCTAssertTrue(detail.hasSuffix("system_reset=1,system_off=1,console_stats=0"), detail)
    }

    func testSystemOffObservedOnlyForTheExactRecordUnderAnyLineSeparator() {
        XCTAssertEqual(HvfStopLine.systemOff, off)
        for separator in ["\n", "\r\n", "\r"] {
            XCTAssertTrue(HvfStopLine.systemOffObserved(in: "exits: 1" + separator + off + separator), separator)
        }
        for text in ["stop: PSCI SYSTEM_OFF\n", "\(off) extra\n", "guest \(off)\n", "stop: PSCI SYSTEM_OFF (system off)\n"] {
            XCTAssertFalse(HvfStopLine.systemOffObserved(in: text), text)
        }
    }
}
