import Foundation
import XCTest
@testable import BridgeVMProductE2E

// The app's pacing records of the journey's first run reach the private T17 and T19 lane results as integer fields
// under first_run_host_records.app_display_pacing and app_ordered_input, beside the host's records, and are absent
// when the app wrote none or the file is anything but its two lines. The samples are the lines the app's Swift suites
// print (HvfDisplayPacingRecordTests, HvfAppPacingRecordsTests); product-e2e-lane-app-pacing-test.py pins the same.
final class T17FirstRunAppPacingTests: XCTestCase {
    typealias Fixture = T17FirstRunHostRecordsTests
    static let display = "app display pacing: ticks=12 presented=6 repeated=6 busy=0 unmapped=0 iosurface=0 "
        + "first_frames=1 skipped=0 period_min_us=15625 period_max_us=15625 missed_vsyncs=0 max_tick_gap_ms=15 "
        + "hold_1=0 hold_2=5 hold_3=0 hold_4=0 hold_5=0 hold_6=0 hold_gt_6=0 age_le_8ms=0 age_le_17ms=0 "
        + "age_le_33ms=5 age_le_50ms=0 age_le_100ms=0 age_gt_100ms=0 age_unknown=0 max_age_us=23437 "
        + "max_work_us=0 stalls_gt_50ms=0 max_stall_ms=0"
    static let input = "app ordered input: queued=3 coalesced=1 legacy=0 refused=0 sent=2 inserted=2 discarded=0 "
        + "depth_1=1 depth_2=2 depth_le_4=0 depth_le_16=0 depth_gt_16=0 max_depth=2 rtt_le_10ms=0 rtt_le_20ms=1 "
        + "rtt_le_50ms=1 rtt_le_100ms=0 rtt_le_250ms=0 rtt_gt_250ms=0 max_rtt_ms=40"
    static let file = display + "\n" + input + "\n"

    private func bundle() throws -> URL {
        let bundle = FileManager.default.temporaryDirectory.appendingPathComponent("bridgevm-app-pacing-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: bundle.appendingPathComponent("logs/hvf"), withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: bundle) }
        return bundle
    }

    func testTheTwoLinesBecomeIntegerFieldsByRecordName() throws {
        let parsed = T17FirstRunAppPacing.parse(Self.file)
        XCTAssertEqual(Set(parsed.keys), ["app_display_pacing", "app_ordered_input"])
        let display = try XCTUnwrap(parsed["app_display_pacing"]), input = try XCTUnwrap(parsed["app_ordered_input"])
        XCTAssertEqual(display.count, 30)
        XCTAssertEqual([display["ticks"], display["hold_2"], display["max_age_us"]], [12, 5, 23437])
        XCTAssertEqual(input.count, 20)
        XCTAssertEqual([input["queued"], input["coalesced"], input["max_rtt_ms"]], [3, 1, 40])
        XCTAssertEqual(T17FirstRunAppPacing.records.map(\.prefix), ["app display pacing:", "app ordered input:"])
    }

    func testAnythingButTheAppsTwoLinesCarriesNothing() {
        let variants = [
            Self.display + "\n", Self.input + "\n" + Self.display + "\n", String(Self.file.dropLast()), Self.file + "x\n",
            Self.file.replacingOccurrences(of: "ticks=12", with: "ticks=012"),
            Self.file.replacingOccurrences(of: " busy=0", with: ""),
            Self.file.replacingOccurrences(of: "discarded=0", with: "discarded=18446744073709551616"),
            Self.file.replacingOccurrences(of: "sent=2", with: "sent=-2"),
            Self.file.replacingOccurrences(of: "repeated=6 busy=0", with: "busy=0 repeated=6"), "",
        ]
        for variant in variants {
            XCTAssertEqual(T17FirstRunAppPacing.parse(variant), [:], variant)
        }
    }

    func testTheJourneyCopiesOnlyAVersionWrittenAfterItsMark() throws {
        let bundle = try bundle()
        let source = bundle.appendingPathComponent(T17FirstRunAppPacing.source)
        let copy = bundle.appendingPathComponent("metadata/product-e2e/\(T17FirstRunAppPacing.archive)")
        try Data("an earlier session\n".utf8).write(to: source)
        let mark = T17FirstRunAppPacing.mark(bundle: bundle)
        XCTAssertNotNil(mark)
        XCTAssertFalse(T17FirstRunAppPacing.archive(since: mark, bundle: bundle, wait: 0.3), "the earlier version is not copied")
        XCTAssertFalse(FileManager.default.fileExists(atPath: copy.path))
        try Data(Self.file.utf8).write(to: source, options: .atomic)  // the app replaces the file as the session ends
        XCTAssertTrue(T17FirstRunAppPacing.archive(since: mark, bundle: bundle, wait: 1))
        XCTAssertEqual(try String(contentsOf: copy, encoding: .utf8), Self.file)
        XCTAssertEqual(T17FirstRunAppPacing.read(bundlePath: bundle.path), T17FirstRunAppPacing.parse(Self.file))
        // A later session's file replaces the source; the first run's copy stays.
        try Data("a later session\n".utf8).write(to: source, options: .atomic)
        XCTAssertFalse(T17FirstRunAppPacing.archive(since: mark, bundle: bundle, wait: 0.3), "an existing copy is kept")
        XCTAssertEqual(try String(contentsOf: copy, encoding: .utf8), Self.file)
    }

    func testALinkedOrMissingFileIsNeitherCopiedNorRead() throws {
        let bundle = try bundle()
        XCTAssertNil(T17FirstRunAppPacing.mark(bundle: bundle))
        XCTAssertFalse(T17FirstRunAppPacing.archive(since: nil, bundle: bundle, wait: 0.2), "no session wrote the file")
        let outside = bundle.appendingPathComponent("outside.log")
        try Data(Self.file.utf8).write(to: outside)
        try FileManager.default.createSymbolicLink(at: bundle.appendingPathComponent(T17FirstRunAppPacing.source),
                                                   withDestinationURL: outside)
        XCTAssertFalse(T17FirstRunAppPacing.archive(since: nil, bundle: bundle, wait: 0.2))
        let copy = bundle.appendingPathComponent("metadata/product-e2e/\(T17FirstRunAppPacing.archive)")
        try FileManager.default.createDirectory(at: copy.deletingLastPathComponent(), withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(at: copy, withDestinationURL: outside)
        XCTAssertEqual(T17FirstRunAppPacing.read(bundlePath: bundle.path), [:])
    }

    func testLaneResultsCarryTheAppRecordsBesideTheHostRecords() throws {
        let bundle = try bundle()
        let evidence = bundle.appendingPathComponent("metadata/product-e2e", isDirectory: true)
        try FileManager.default.createDirectory(at: evidence, withIntermediateDirectories: true)
        struct Lane: Encodable { let lane = 1 }
        XCTAssertNil(try written(Lane(), bundle, "none.json"), "neither file: no key, as before")
        try Data(Self.file.utf8).write(to: evidence.appendingPathComponent(T17FirstRunAppPacing.archive))
        let alone = try XCTUnwrap(written(Lane(), bundle, "alone.json"))
        XCTAssertEqual(alone, T17FirstRunAppPacing.parse(Self.file))
        let hda = Fixture.records(Fixture.poll, Fixture.stream, Fixture.repeats, Fixture.timing)
        try Data(Fixture.report(tail: hda + Fixture.teardown).utf8)
            .write(to: evidence.appendingPathComponent(T17FirstRunHostRecords.archive))
        let both = try XCTUnwrap(written(Lane(), bundle, "both.json"))
        XCTAssertEqual(both, Fixture.expected.merging(T17FirstRunAppPacing.parse(Self.file)) { host, _ in host })
    }

    private func written<Lane: Encodable>(_ lane: Lane, _ bundle: URL, _ name: String) throws -> [String: [String: UInt64]]? {
        let url = bundle.appendingPathComponent(name)
        try T17ResultWriter.write(T17FirstRunHostRecords.attach(lane, bundlePath: bundle.path), to: url)
        return try JSONDecoder().decode(Carried.self, from: Data(contentsOf: url)).records
    }

    private struct Carried: Decodable {
        let records: [String: [String: UInt64]]?
        enum CodingKeys: String, CodingKey { case records = "first_run_host_records" }
    }
}
