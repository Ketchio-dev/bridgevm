import Foundation
import XCTest
@testable import BridgeVMProductE2E

// The first run's `ramfb display timing:` record reaches the private T17 and T19 lane
// results as integer fields under first_run_host_records.ramfb_display_timing, beside
// the HDA records (T17FirstRunHostRecordsTests.swift), and is absent when the platform
// printed none. product-e2e-lane-display-records-test.py pins the same values.
final class T17FirstRunDisplayRecordsTests: XCTestCase {
    typealias Fixture = T17FirstRunHostRecordsTests
    static let expected: [String: UInt64] = [
        "ticks": 4960, "late_le_1ms": 4100, "late_le_4ms": 700, "late_le_8ms": 120, "late_le_16ms": 30,
        "late_gt_16ms": 10, "max_late_us": 41250, "published": 3202, "interval_le_20ms": 0, "interval_le_40ms": 2900,
        "interval_le_70ms": 250, "interval_gt_70ms": 51, "max_interval_ms": 9050,
    ]
    static let hda = Fixture.records(Fixture.poll, Fixture.stream, Fixture.repeats, Fixture.timing)
    static let line = HvfHostDisplayTests.line

    private func parsed(_ tail: String) -> [String: [String: UInt64]] {
        guard let bound = HvfTerminalReport.hostTail(in: Data(Fixture.report(tail: tail).utf8)) else { return ["unbound": [:]] }
        return HvfHostRecordFields.parse(tail: bound)
    }

    func testTheDisplayRecordBecomesIntegerFieldsBesideTheHdaRecords() {
        var expected = Fixture.expected
        expected["ramfb_display_timing"] = Self.expected
        XCTAssertEqual(parsed(Self.hda + Fixture.teardown + Self.line), expected)
        XCTAssertEqual(parsed(Self.line), ["ramfb_display_timing": Self.expected])
        XCTAssertEqual(parsed(Self.hda + Fixture.teardown), Fixture.expected, "no display record, as before")
        XCTAssertEqual(HvfHostRecordFields.records.map(\.prefix),
                       HvfHostTail.diagnosticRecords.map(\.prefix) + [HvfHostTail.displayRecord.prefix])
    }

    func testAMisplacedOrSecondDisplayRecordCarriesNothing() {
        XCTAssertEqual(parsed(Self.line + Self.hda + Fixture.teardown), ["unbound": [:]])
        XCTAssertEqual(parsed(Self.hda + Fixture.teardown + Self.line + Self.line), ["unbound": [:]])
        let beyond = Self.line.replacingOccurrences(of: "ticks=4960", with: "ticks=18446744073709551616")
        XCTAssertEqual(parsed(Self.hda + Fixture.teardown + beyond), Fixture.expected, "a value beyond UInt64 drops the record")
    }

    func testT17AndT19LaneResultsCarryTheDisplayRecord() throws {
        let bundle = FileManager.default.temporaryDirectory.appendingPathComponent("bridgevm-display-\(UUID().uuidString)")
        addTeardownBlock { try? FileManager.default.removeItem(at: bundle) }
        let evidence = bundle.appendingPathComponent("metadata/product-e2e", isDirectory: true)
        try FileManager.default.createDirectory(at: evidence, withIntermediateDirectories: true)
        try Data(Fixture.report(tail: Self.hda + Fixture.teardown + Self.line).utf8)
            .write(to: evidence.appendingPathComponent(T17FirstRunHostRecords.archive))
        let nonce = String(repeating: "a", count: 64), t17Evidence = T17Evidence(nonce: nonce)
        let t17 = T17LaneResult(
            jobID: "fixture", commit: String(repeating: "b", count: 40), campaignMode: "pilot", lane: 1, nonce: nonce,
            uiFrontendAutomated: true, failureCode: "none", failureDetail: "", cleanupVerified: true,
            installerSourcePath: "/private/tmp/source.raw", stages: t17Evidence.stages, hashes: t17Evidence.hashes)
        let fixture = try A9ImportRequestFixture()
        let t19 = A9ImportEvidence(nonce: nonce).result(
            request: try A9ImportRequest.load(fixture.requestURL), failureCode: "guest-evidence-missing",
            failureDetail: "detail", cleanupVerified: true, uiFrontendAutomated: true)
        let carried = [try written(t17, bundle, "t17.json"), try written(t19, bundle, "t19.json")]
        for records in carried {
            XCTAssertEqual(records["ramfb_display_timing"], Self.expected)
            XCTAssertEqual(Set(records.keys), Set(Fixture.expected.keys).union(["ramfb_display_timing"]))
        }
    }

    private func written<Lane: Encodable>(_ lane: Lane, _ bundle: URL, _ name: String) throws -> [String: [String: UInt64]] {
        let url = bundle.appendingPathComponent(name)
        try T17ResultWriter.write(T17FirstRunHostRecords.attach(lane, bundlePath: bundle.path), to: url)
        return try JSONDecoder().decode(Carried.self, from: Data(contentsOf: url)).records
    }

    private struct Carried: Decodable {
        let records: [String: [String: UInt64]]
        enum CodingKeys: String, CodingKey { case records = "first_run_host_records" }
    }
}
