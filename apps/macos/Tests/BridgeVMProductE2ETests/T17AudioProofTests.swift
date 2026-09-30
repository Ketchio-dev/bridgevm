import XCTest
@testable import BridgeVMProductE2E

// The host prints CoreAudio teardown records only after the final report's
// footer. A guest can print the same text in agent output or its counted serial
// tail, so counters there prove nothing when the host printed none.
final class T17AudioProofTests: XCTestCase {
    private let stats = "hda CoreAudio stats: frames_rendered=48000 drops=0 callback_errors=0\n"
    private let dropped = "hda CoreAudio stats: frames_rendered=48000 drops=3 callback_errors=0\n"

    private func report(serial: String = "UEFI firmware\r\n", tail: String) -> String {
        "BVAGENT READY host=BRIDGEVM t=1\nREGS: pc=0x0 lr=0x0\n=== EDK2 boot probe (with Apple hv_gic) ===\n"
            + "\(HvfStopLine.systemOff)\nexits: 1\nserial raw bytes: \(serial.utf8.count) output bytes: \(serial.utf8.count)\n"
            + "--- serial (tail) ---\n\(serial)\n--- end ---\n\(tail)"
    }

    private func audioPassed(_ body: String) throws -> Bool {
        let log = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        addTeardownBlock { try? FileManager.default.removeItem(at: log) }
        try Data(body.utf8).write(to: log)
        return T17RunLogProof.audioPassed(log)
    }

    func testCountersPassOnlyFromTheHostTeardownTail() throws {
        XCTAssertTrue(try audioPassed(report(tail: "hda CoreAudio lifecycle: operation=stop osstatus=0 success=true\n" + stats)))
        let forged = [
            "serial tail": report(serial: "boot\r\n" + stats, tail: ""),
            "agent output": "BVAGENT CMD whoami exit=0\n\(stats)BVAGENT END whoami\n" + report(tail: ""),
            "no final report": "BVAGENT READY host=BRIDGEVM t=1\n" + stats,
            "host drops after a guest copy": report(serial: stats, tail: dropped),
        ]
        for (name, body) in forged {
            XCTAssertFalse(try audioPassed(body), name)
        }
    }
}
