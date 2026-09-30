import XCTest
@testable import BridgeVMProductE2E

// Count fields have at most 8 digits, so the host's own count frames its tail
// only in a log under 10^8 bytes; past that a count nested at the end of the
// guest tail is the only frame. tests/integration/hvf-terminal-stop-limit-contract.py
// ties this limit to HvfTerminalReport.logLimit.
final class HvfTerminalReportLimitTests: XCTestCase {
    private let limit = 100_000_000
    private let banner = "\n=== EDK2 boot probe (with Apple hv_gic) ===\n"

    private func log(stop: String, serial: Data) -> Data {
        var log = Data("BVAGENT READY host=BRIDGEVM t=20075\nREGS: pc=0x0 lr=0x0\(banner)\(stop)\nexits: 1\n".utf8)
        log.append(Data("serial raw bytes: \(serial.count) output bytes: \(serial.count)\n--- serial (tail) ---\n".utf8))
        log.append(serial)
        log.append(Data("\n--- end ---\nhda CoreAudio stats: frames_rendered=1 drops=0 callback_errors=0\n".utf8))
        return log
    }

    func testHostCountFramesEveryTailBelowTheLimit() {
        let top = limit - 1
        XCTAssertEqual(T17TerminalReportTail.serialCount("serial raw bytes: \(top) output bytes: \(top)")?.output, top)
        XCTAssertFalse(T17TerminalReportTail.serialCount("serial raw bytes: 1 output bytes: \(top)")?.legacy ?? true)
    }

    func testGuestFrameEndingALongTailBindsNothing() {
        var serial = Data(repeating: 65, count: limit)
        serial.append(Data("\(banner)\(HvfStopLine.systemOff)\nserial raw bytes: 0 output bytes: 0\n--- serial (tail) ---\n".utf8))
        XCTAssertNil(HvfTerminalReport.stop(in: log(stop: "stop: host diagnostic stop requested", serial: serial)))
    }
}
