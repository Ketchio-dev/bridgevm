import Foundation
import XCTest
@testable import BridgeVMProductE2E

// The HDA controller prints four diagnostic records just before its CoreAudio
// sink's lifecycle, stats and continuity records. Both Swift tail readers admit
// each at most once, in print order and before the stats record, within the
// unchanged 16-record and 4 KiB bounds; a log from before them reads as it did.
// tests/integration/hda-diagnostics-host-tail-contract.py runs the same cases on
// the Python and shell twins and ties the fields to the Rust printers.
final class HvfHostDiagnosticsTests: XCTestCase {
    private let poll = "hda poll: polls=3 gap_le_10ms=2 gap_le_20ms=0 gap_le_50ms=0 gap_le_100ms=0 gap_gt_100ms=1 "
        + "max_gap_ms=250 capped_polls=1 capped_lost_ms=150 ioc_completions=11 max_iocs_per_poll=10 msis_sent=1"
    private let stream = "hda stream: runs=1 rate_hz=48000 channels=2 bits=16 cbl_bytes=3840 cbl_ms=20.000 entries=2 "
        + "entry_bytes_min=1920 entry_bytes_max=1920 entry_ms_min=10.000 entry_ms_max=10.000 ioc_entries=2 "
        + "position_buffer=1 lpib_reads=7 running_ms=260"
    private let repeats = "hda repeat: passes=12 silent_passes=2 compared_passes=8 repeat_passes=3 max_repeat_run=2"
    private let timing = "hda CoreAudio timing: callbacks=10 interval_le_10ms=1 interval_le_15ms=2 interval_le_20ms=1 "
        + "interval_le_30ms=2 interval_le_50ms=2 interval_gt_50ms=1 max_interval_ms=51 late_callbacks=3 "
        + "trylock_misses=0 ring_samples=4 ring_min_ms=30 ring_max_ms=50 ring_final_ms=30"
    private let continuity = "hda CoreAudio continuity: active_callbacks=412 underrun_callbacks=9 underrun_frames=2160 "
        + "contention_callbacks=1 gaps=4 max_gap_frames=960 stream_stops=1 callback_frames=480\n"
    private let lifecycle = "hda CoreAudio lifecycle: operation=stop osstatus=0 success=true\n"
        + "hda CoreAudio lifecycle: operation=dispose osstatus=0 success=true\n"
    private let stats = T17AudioCountersTests.line() + "\n"

    private func report(tail: String) -> String {
        "BVAGENT READY host=BRIDGEVM t=1\nREGS: pc=0x0 lr=0x0\n=== EDK2 boot probe (with Apple hv_gic) ===\n"
            + "\(HvfStopLine.systemOff)\nexits: 1\nserial raw bytes: 15 output bytes: 15\n"
            + "--- serial (tail) ---\nUEFI firmware\r\n\n--- end ---\n\(tail)"
    }

    private func lines(_ records: String...) -> String { records.map { $0 + "\n" }.joined() }

    private var cases: [String: (tail: String, admitted: Bool)] {
        let teardown = lifecycle + stats + continuity, all = lines(poll, stream, repeats, timing)
        return [
            "the four records before the teardown records": (all + teardown, true),
            "no records, as every log before them": (teardown, true),
            "the timing record alone": (lines(timing) + teardown, true),
            "a record between the stats and continuity": (lifecycle + stats + lines(poll) + continuity, false),
            "a record after the continuity record": (teardown + lines(poll), false),
            "two poll records": (lines(poll) + all + teardown, false),
            "records out of print order": (lines(stream, poll) + teardown, false),
            "a missing field": (lines(poll.replacingOccurrences(of: " msis_sent=1", with: "")) + teardown, false),
            "a leading zero": (lines(poll.replacingOccurrences(of: "gap_le_10ms=2", with: "gap_le_10ms=02")) + teardown, false),
            "whole milliseconds": (lines(stream.replacingOccurrences(of: "cbl_ms=20.000", with: "cbl_ms=20")) + teardown, false),
            "a position buffer of 2": (lines(stream.replacingOccurrences(of: "position_buffer=1", with: "position_buffer=2"))
                + teardown, false),
            "a 21-digit value": (lines(timing.replacingOccurrences(of: "callbacks=10", with: "callbacks="
                + String(repeating: "1", count: 21))) + teardown, false),
            "a carriage return": (timing + "\r\n" + teardown, false),
        ]
    }

    /// Every field at its widest, three stopping enqueue failures (one per
    /// AudioQueue buffer) and failed stop and dispose: 11 records, 3,211 bytes.
    private var widest: String {
        let max = "18446744073709551615"
        let value = { (field: String) in
            field == "position_buffer" ? "1" : HvfHostTail.diagnosticFieldValues[field] == nil ? max : max + ".999"
        }
        let diagnostics = HvfHostTail.diagnosticRecords.map { record in
            record.prefix + record.fields.map { " \($0)=\(value($0))" }.joined() + "\n"
        }.joined()
        let enqueue = String(repeating: "hda CoreAudio callback enqueue: state=stopping "
            + "reason=stopping-enqueue-during-reset osstatus=-2147483648 expected=true\n", count: 3)
        let failed = ["stop", "dispose"].map { "hda CoreAudio lifecycle: operation=\($0) osstatus=-2147483648 success=false\n" }
        return diagnostics + enqueue + failed.joined() + "hda CoreAudio stats:" + T17AudioCounters.fields.map { " \($0)=\(max)" }.joined()
            + "\nhda CoreAudio continuity:" + HvfHostTail.continuityFields.map { " \($0)=\(max)" }.joined() + "\n"
    }

    func testBothTailReadersAdmitTheRecordsOnceEachBeforeTheTeardownRecords() {
        for (name, value) in cases {
            XCTAssertEqual(HvfHostTail.isBoundedShutdownTail(value.tail), value.admitted, name)
            XCTAssertEqual(HvfTerminalReport.stop(in: report(tail: value.tail))?.line == HvfStopLine.systemOff,
                           value.admitted, name)
        }
    }

    func testTheWidestTailFitsTheUnchangedBounds() {
        XCTAssertEqual(widest.split(separator: "\n").count, 11)
        XCTAssertEqual(widest.utf8.count, 3211)
        XCTAssertTrue(HvfHostTail.isBoundedShutdownTail(widest))
        XCTAssertEqual(HvfTerminalReport.stop(in: report(tail: widest))?.line, HvfStopLine.systemOff)
    }

    func testT17AudioCheckStillReadsTheStatsRecord() throws {
        for (name, value) in cases {
            let log = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
            addTeardownBlock { try? FileManager.default.removeItem(at: log) }
            try Data(report(tail: value.tail).utf8).write(to: log)
            XCTAssertEqual(T17RunLogProof.audioPassed(log), value.admitted, name)
        }
    }

    func testPlacementKeepsPrintOrderBeforeTheTeardownRecords() {
        let stats = "hda CoreAudio stats: a=1", continuity = "hda CoreAudio continuity: x"
        XCTAssertTrue(HvfHostTail.continuityPlaced(["hda poll: x", "hda CoreAudio timing: x", stats, continuity]))
        XCTAssertFalse(HvfHostTail.continuityPlaced(["hda repeat: x", "hda stream: x", stats]))
        XCTAssertFalse(HvfHostTail.continuityPlaced([stats, continuity, "hda poll: x"]))
        XCTAssertEqual(HvfHostTail.diagnosticRecords.map(\.fields.count), [12, 15, 5, 14])
    }
}
