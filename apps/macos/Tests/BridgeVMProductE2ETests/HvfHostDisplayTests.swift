import Foundation
import XCTest
@testable import BridgeVMProductE2E

/// ramfb-display-timing-host-tail-contract.py's cases on the Swift twins: the bound
/// final report (HvfTerminalReport.hostTail) and T17's bounded audio tail admit one
/// `ramfb display timing:` record after every HDA record and refuse it anywhere else,
/// twice or malformed; a tail without it reads as before.
final class HvfHostDisplayTests: XCTestCase {
    typealias Fixture = T17FirstRunHostRecordsTests
    static let display = "ramfb display timing: ticks=4960 late_le_1ms=4100 late_le_4ms=700 late_le_8ms=120 "
        + "late_le_16ms=30 late_gt_16ms=10 max_late_us=41250 published=3202 interval_le_20ms=0 "
        + "interval_le_40ms=2900 interval_le_70ms=250 interval_gt_70ms=51 max_interval_ms=9050"
    static let line = display + "\n"
    static let hda = Fixture.records(Fixture.poll, Fixture.stream, Fixture.repeats, Fixture.timing)

    /// Whether the bound final report and T17's audio tail admit `tail`.
    private func admitted(_ tail: String) -> [Bool] {
        [HvfTerminalReport.hostTail(in: Data(Fixture.report(tail: tail).utf8)) != nil, HvfHostTail.isBoundedShutdownTail(tail)]
    }

    func testOneRecordAfterEveryHdaRecordIsAdmitted() {
        let widest = HvfHostTail.displayRecord.prefix
            + HvfHostTail.displayRecord.fields.map { " \($0)=18446744073709551615" }.joined() + "\n"
        let tails = [Self.hda + Fixture.teardown, Self.hda + Fixture.teardown + Self.line, Self.hda + Self.line, Self.line,
                     Self.hda + Fixture.teardown + widest]
        for tail in tails { XCTAssertEqual(admitted(tail), [true, true], tail) }
        XCTAssertEqual(HvfTerminalReport.hostTail(in: Data(Fixture.report(tail: Self.hda + Fixture.teardown + Self.line).utf8)),
                       Data((Self.hda + Fixture.teardown + Self.line).utf8))
    }

    func testTheRecordAnywhereElseTwiceOrMalformedIsRefused() {
        let malformed = [Self.display.replacingOccurrences(of: " published=3202", with: ""), Self.display + " dropped=0",
                         Self.display.replacingOccurrences(of: "ticks=4960", with: "ticks=04960"),
                         Self.display.replacingOccurrences(of: "ticks=4960", with: "ticks=" + String(repeating: "1", count: 21)),
                         Self.display.replacingOccurrences(of: "max_late_us=41250", with: "max_late_us=41.250"), Self.display + " "]
        let tails = [Self.line + Self.hda + Fixture.teardown, Self.hda + Fixture.teardown + Self.line + Self.line,
                     Fixture.teardown + Self.line + Fixture.poll + "\n", Self.hda + Self.line + T17AudioCountersTests.line() + "\n"]
            + malformed.map { Fixture.teardown + $0 + "\n" }
        for tail in tails { XCTAssertEqual(admitted(tail), [false, false], tail) }
    }

    func testThePlacementRuleMatchesThePythonTwin() {
        let (stats, record) = ("hda CoreAudio stats: x", "ramfb display timing: x")
        XCTAssertTrue(HvfHostTail.continuityPlaced(["hda poll: x", stats, "hda CoreAudio continuity: x", record]))
        XCTAssertTrue(HvfHostTail.continuityPlaced([record, "hda CoreAudio lifecycle: x"]), "uncoded records stay free")
        XCTAssertFalse(HvfHostTail.continuityPlaced([record, "hda poll: x"]))
        XCTAssertFalse(HvfHostTail.continuityPlaced([stats, record, stats]))
        XCTAssertFalse(HvfHostTail.continuityPlaced([record, record]))
    }
}
