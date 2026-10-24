import Foundation
import XCTest
@testable import BridgeVMProductE2E

// The journey archives its first boot's run log, where the guest played audio and
// took the keyboard and pointer challenge, as metadata/product-e2e/first-run.log.
// The HDA diagnostic records of its bound post-footer host tail reach the private
// T17 and T19 lane results as integer fields under first_run_host_records, and the
// key is absent when the host printed none.
final class T17FirstRunHostRecordsTests: XCTestCase {
    static let poll = "hda poll: polls=3 gap_le_10ms=2 gap_le_20ms=0 gap_le_50ms=0 gap_le_100ms=0 gap_gt_100ms=1 "
        + "max_gap_ms=250 capped_polls=1 capped_lost_ms=150 ioc_completions=11 max_iocs_per_poll=10 msis_sent=1"
    static let stream = "hda stream: runs=1 rate_hz=48000 channels=2 bits=16 cbl_bytes=3840 cbl_ms=21.333 entries=2 "
        + "entry_bytes_min=1920 entry_bytes_max=1920 entry_ms_min=10.000 entry_ms_max=10.666 ioc_entries=2 "
        + "position_buffer=1 lpib_reads=7 running_ms=260"
    static let repeats = "hda repeat: passes=12 silent_passes=2 compared_passes=8 repeat_passes=3 max_repeat_run=2"
    static let timing = "hda CoreAudio timing: callbacks=10 interval_le_10ms=1 interval_le_15ms=2 interval_le_20ms=1 "
        + "interval_le_30ms=2 interval_le_50ms=2 interval_gt_50ms=1 max_interval_ms=51 late_callbacks=3 "
        + "trylock_misses=0 ring_samples=4 ring_min_ms=30 ring_max_ms=50 ring_final_ms=18446744073709551615"
    static let teardown = "hda CoreAudio lifecycle: operation=stop osstatus=0 success=true\n" + T17AudioCountersTests.line()
        + "\nhda CoreAudio continuity: active_callbacks=412 underrun_callbacks=9 underrun_frames=2160 "
        + "contention_callbacks=1 gaps=4 max_gap_frames=960 stream_stops=1 callback_frames=480\n"
    static let expected: [String: [String: UInt64]] = [
        "hda_poll": ["polls": 3, "gap_le_10ms": 2, "gap_le_20ms": 0, "gap_le_50ms": 0, "gap_le_100ms": 0,
                     "gap_gt_100ms": 1, "max_gap_ms": 250, "capped_polls": 1, "capped_lost_ms": 150,
                     "ioc_completions": 11, "max_iocs_per_poll": 10, "msis_sent": 1],
        "hda_stream": ["runs": 1, "rate_hz": 48000, "channels": 2, "bits": 16, "cbl_bytes": 3840, "cbl_us": 21333,
                       "entries": 2, "entry_bytes_min": 1920, "entry_bytes_max": 1920, "entry_us_min": 10000,
                       "entry_us_max": 10666, "ioc_entries": 2, "position_buffer": 1, "lpib_reads": 7, "running_ms": 260],
        "hda_repeat": ["passes": 12, "silent_passes": 2, "compared_passes": 8, "repeat_passes": 3, "max_repeat_run": 2],
        "hda_coreaudio_timing": ["callbacks": 10, "interval_le_10ms": 1, "interval_le_15ms": 2, "interval_le_20ms": 1,
                                 "interval_le_30ms": 2, "interval_le_50ms": 2, "interval_gt_50ms": 1,
                                 "max_interval_ms": 51, "late_callbacks": 3, "trylock_misses": 0, "ring_samples": 4,
                                 "ring_min_ms": 30, "ring_max_ms": 50, "ring_final_ms": UInt64.max],
    ]

    static func report(serial: String = "UEFI firmware\r\n", tail: String) -> String {
        "BVAGENT READY host=BRIDGEVM t=1\nREGS: pc=0x0 lr=0x0\n=== EDK2 boot probe (with Apple hv_gic) ===\n"
            + "\(HvfStopLine.systemOff)\nexits: 1\nserial raw bytes: \(serial.utf8.count) output bytes: \(serial.utf8.count)\n"
            + "--- serial (tail) ---\n\(serial)\n--- end ---\n\(tail)"
    }

    static func records(_ lines: String...) -> String { lines.map { $0 + "\n" }.joined() }

    private func parsed(_ log: String) -> [String: [String: UInt64]] {
        guard let tail = HvfTerminalReport.hostTail(in: Data(log.utf8)) else { return ["unbound": [:]] }
        return HvfHostRecordFields.parse(tail: tail)
    }

    /// A lane bundle whose first run log holds `log`, or none when `log` is nil.
    private func bundle(_ log: String?) throws -> URL {
        let bundle = FileManager.default.temporaryDirectory.appendingPathComponent("bridgevm-first-run-\(UUID().uuidString)")
        addTeardownBlock { try? FileManager.default.removeItem(at: bundle) }
        let evidence = bundle.appendingPathComponent("metadata/product-e2e", isDirectory: true)
        try FileManager.default.createDirectory(at: evidence, withIntermediateDirectories: true)
        if let log { try Data(log.utf8).write(to: evidence.appendingPathComponent(T17FirstRunHostRecords.archive)) }
        return bundle
    }

    func testTheFourRecordsBecomeIntegerFieldsWithMillisecondsAsMicroseconds() {
        let tail = Self.records(Self.poll, Self.stream, Self.repeats, Self.timing) + Self.teardown
        XCTAssertEqual(parsed(Self.report(tail: tail)), Self.expected)
        XCTAssertEqual(parsed(Self.report(tail: Self.records(Self.timing) + Self.teardown)),
                       ["hda_coreaudio_timing": Self.expected["hda_coreaudio_timing"]!])
        XCTAssertEqual(HvfHostRecordFields.records.map(\.prefix), HvfHostTail.diagnosticRecords.map(\.prefix) + [HvfHostTail.displayRecord.prefix])
    }

    func testRecordsTheHostDidNotPrintAreAbsent() {
        XCTAssertEqual(parsed(Self.report(tail: Self.teardown)), [:])
        XCTAssertEqual(parsed(Self.report(tail: "")), [:])
    }

    func testOnlyTheBoundHostTailCounts() {
        let copies = Self.records(Self.poll, Self.stream, Self.repeats, Self.timing)
        XCTAssertEqual(parsed(Self.report(serial: copies, tail: Self.teardown)), [:], "a guest copy in the serial tail")
        XCTAssertEqual(parsed("BVAGENT CMD x exit=0\n" + copies + "BVAGENT END x\n" + Self.report(tail: "")), [:])
        XCTAssertEqual(HvfHostRecordFields.parse(tail: Data((copies + Self.teardown).utf8)).count, 4, "a bound tail")
        XCTAssertNil(HvfTerminalReport.hostTail(in: Data(("no final report\n" + copies).utf8)))
    }

    func testAValueBeyondUInt64DropsOnlyItsRecord() {
        let beyond = Self.repeats.replacingOccurrences(of: "passes=12", with: "passes=18446744073709551616")
        let tail = Self.records(Self.poll, Self.stream, beyond, Self.timing) + Self.teardown
        var expected = Self.expected
        expected["hda_repeat"] = nil
        XCTAssertEqual(parsed(Self.report(tail: tail)), expected)
    }

    func testTheArchivedFirstRunLogIsReadAndAnythingElseCarriesNothing() throws {
        let tail = Self.records(Self.poll, Self.stream, Self.repeats, Self.timing) + Self.teardown
        XCTAssertEqual(T17FirstRunHostRecords.read(bundlePath: try bundle(Self.report(tail: tail)).path), Self.expected)
        XCTAssertEqual(T17FirstRunHostRecords.read(bundlePath: try bundle(nil).path), [:], "no first run")
        XCTAssertEqual(T17FirstRunHostRecords.read(bundlePath: try bundle("BVAGENT READY\n").path), [:], "no final report")
        let linked = try bundle(nil), real = try bundle(Self.report(tail: tail))
        try FileManager.default.createSymbolicLink(
            at: linked.appendingPathComponent("metadata/product-e2e/\(T17FirstRunHostRecords.archive)"),
            withDestinationURL: real.appendingPathComponent("metadata/product-e2e/\(T17FirstRunHostRecords.archive)"))
        XCTAssertEqual(T17FirstRunHostRecords.read(bundlePath: linked.path), [:], "a symlink")
    }

    func testT17AndT19LaneResultsCarryTheRecordsOnlyWhenPrinted() throws {
        let nonce = String(repeating: "a", count: 64), evidence = T17Evidence(nonce: nonce)
        let t17 = T17LaneResult(
            jobID: "fixture", commit: String(repeating: "b", count: 40), campaignMode: "pilot", lane: 1, nonce: nonce,
            uiFrontendAutomated: true, failureCode: "none", failureDetail: "", cleanupVerified: true,
            installerSourcePath: "/private/tmp/source.raw", stages: evidence.stages, hashes: evidence.hashes)
        let fixture = try A9ImportRequestFixture()
        let t19 = A9ImportEvidence(nonce: nonce).result(
            request: try A9ImportRequest.load(fixture.requestURL), failureCode: "guest-evidence-missing",
            failureDetail: "detail", cleanupVerified: true, uiFrontendAutomated: true)
        let tail = Self.records(Self.poll, Self.stream, Self.repeats, Self.timing) + Self.teardown
        let printed = try bundle(Self.report(tail: tail)).path, silent = try bundle(Self.report(tail: Self.teardown)).path
        try check(t17, printed: printed, silent: silent)
        try check(t19, printed: printed, silent: silent)
    }

    private func check<Lane: Encodable>(_ lane: Lane, printed: String, silent: String) throws {
        let directory = try bundle(nil)
        func written<Result: Encodable>(_ result: Result, _ name: String) throws -> Data {
            let url = directory.appendingPathComponent(name)
            try T17ResultWriter.write(result, to: url)
            return try Data(contentsOf: url)
        }
        let base = try written(lane, "base.json")
        XCTAssertEqual(try written(T17FirstRunHostRecords.attach(lane, bundlePath: silent), "silent.json"), base,
                       "no records: the lane result is byte-identical")
        XCTAssertEqual(try written(T17FirstRunHostRecords.attach(lane, bundlePath: directory.path + "/absent"), "absent.json"),
                       base, "no first run log")
        let data = try written(T17FirstRunHostRecords.attach(lane, bundlePath: printed), "printed.json")
        XCTAssertEqual(try JSONDecoder().decode(Carried.self, from: data).records, Self.expected)
        var object = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        XCTAssertNotNil(object.removeValue(forKey: T17FirstRunHostRecords.key))
        XCTAssertEqual(object as NSDictionary, try JSONSerialization.jsonObject(with: base) as? NSDictionary,
                       "every other field is unchanged")
    }

    private struct Carried: Decodable {
        let records: [String: [String: UInt64]]
        enum CodingKeys: String, CodingKey { case records = "first_run_host_records" }
    }
}
