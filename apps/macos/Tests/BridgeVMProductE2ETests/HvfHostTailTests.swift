import Foundation
import XCTest
@testable import BridgeVMProductE2E

// hda_coreaudio_continuity.rs prints one continuity record after the stats
// record. Both Swift tail readers admit at most one well-formed record, after a
// stats record, within the unchanged 16-record and 4 KiB bounds; a log from
// before the record reads as it did. tests/integration/hda-continuity-host-tail-contract.py
// runs the same cases on the Python and shell twins and ties the fields to the printer.
final class HvfHostTailTests: XCTestCase {
    private let continuity = "hda CoreAudio continuity: active_callbacks=412 underrun_callbacks=9 underrun_frames=2160 "
        + "contention_callbacks=1 gaps=4 max_gap_frames=960 stream_stops=1 callback_frames=480"
    private let lifecycle = "hda CoreAudio lifecycle: operation=stop osstatus=0 success=true\n"
        + "hda CoreAudio lifecycle: operation=dispose osstatus=0 success=true\n"
    private let stats = T17AudioCountersTests.line() + "\n"
    private let short = "hda CoreAudio stats: frames_rendered=1 drops=0 callback_errors=0\n"
    private let virgl = "Sep  1 12:57:02  virgl_render_server[43160] <Debug>: socket disconnected\n"

    private func report(tail: String) -> String {
        "BVAGENT READY host=BRIDGEVM t=1\nREGS: pc=0x0 lr=0x0\n=== EDK2 boot probe (with Apple hv_gic) ===\n"
            + "\(HvfStopLine.systemOff)\nexits: 1\nserial raw bytes: 15 output bytes: 15\n"
            + "--- serial (tail) ---\nUEFI firmware\r\n\n--- end ---\n\(tail)"
    }

    private func padded(_ size: Int) -> String {
        let head = "hda CoreAudio stats: a="
        return head + String(repeating: "1", count: size - head.utf8.count - continuity.utf8.count - 2) + "\n" + continuity + "\n"
    }

    private var audioCases: [String: (tail: String, admitted: Bool)] {
        let record = continuity + "\n"
        return [
            "no record, as every log before it": (lifecycle + stats, true),
            "one record after the stats": (lifecycle + stats + record, true),
            "a record before the stats": (lifecycle + record + stats, false),
            "a record without stats": (lifecycle + record, false),
            "two records": (lifecycle + stats + record + record, false),
            "a missing field": (lifecycle + stats + continuity.replacingOccurrences(of: " stream_stops=1", with: "") + "\n", false),
            "a leading zero": (lifecycle + stats + continuity.replacingOccurrences(of: "gaps=4", with: "gaps=04") + "\n", false),
            "a 21-digit value": (lifecycle + stats + continuity.replacingOccurrences(
                of: "active_callbacks=412", with: "active_callbacks=" + String(repeating: "1", count: 21)) + "\n", false),
            "a trailing space": (lifecycle + stats + continuity + " \n", false),
            "a carriage return": (lifecycle + stats + continuity + "\r\n", false),
        ]
    }

    private var boundCases: [String: (tail: String, admitted: Bool)] {
        [
            "sixteen records with the continuity record": (String(repeating: short, count: 15) + continuity + "\n", true),
            "seventeen records with the continuity record": (String(repeating: short, count: 16) + continuity + "\n", false),
            "4096 bytes with the continuity record": (padded(4096), true),
            "4097 bytes with the continuity record": (padded(4097), false),
        ]
    }

    func testBothTailReadersAdmitOneRecordAfterTheStats() {
        XCTAssertEqual(padded(4096).utf8.count, 4096)
        for (name, value) in audioCases.merging(boundCases, uniquingKeysWith: { first, _ in first }) {
            XCTAssertEqual(HvfHostTail.isBoundedShutdownTail(value.tail), value.admitted, name)
            XCTAssertEqual(HvfTerminalReport.stop(in: report(tail: value.tail))?.line == HvfStopLine.systemOff,
                           value.admitted, name)
        }
    }

    func testOnlyTheHvfBindingAdmitsTheRenderServerLineAroundTheRecord() {
        let record = continuity + "\n"
        for (tail, admitted) in [(lifecycle + stats + record + virgl, true), (lifecycle + stats + virgl + record, true),
                                 (lifecycle + stats + virgl + record + record, false)] {
            XCTAssertEqual(HvfTerminalReport.stop(in: report(tail: tail)) != nil, admitted, tail)
            XCTAssertFalse(HvfHostTail.isBoundedShutdownTail(tail), tail)
        }
    }

    func testT17AudioCheckStillReadsTheStatsRecord() throws {
        for (name, value) in audioCases {
            let log = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
            addTeardownBlock { try? FileManager.default.removeItem(at: log) }
            try Data(report(tail: value.tail).utf8).write(to: log)
            XCTAssertEqual(T17RunLogProof.audioPassed(log), value.admitted, name)
        }
    }

    func testPlacementNeedsAPriorStatsRecordAndAtMostOneRecord() {
        let stats = "hda CoreAudio stats: a=1", record = "hda CoreAudio continuity: x"
        XCTAssertTrue(HvfHostTail.continuityPlaced([String]()))
        XCTAssertTrue(HvfHostTail.continuityPlaced([stats, record, stats]))
        XCTAssertFalse(HvfHostTail.continuityPlaced([record, stats]))
        XCTAssertFalse(HvfHostTail.continuityPlaced([stats, record, record]))
        XCTAssertEqual(HvfHostTail.continuityFields.count, 8)
    }
}
