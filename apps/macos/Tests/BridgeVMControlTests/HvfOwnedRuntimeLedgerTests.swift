import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfOwnedRuntimeLedgerTests: XCTestCase {
    func ledger() throws -> HvfOwnedRuntimeLedger {
        let event = try HvfOwnedRuntimeCodecTests.event("ready")
        return HvfOwnedRuntimeLedger(identity: .init(token: try XCTUnwrap(UUID(uuidString: event.runToken)),
            processID: event.runnerPID), manifestSHA256: try XCTUnwrap(event.ready).manifestSHA256)
    }
    func admit(_ ledger: inout HvfOwnedRuntimeLedger) throws {
        let stop = try HvfOwnedRuntimeCodec.decode(HvfOwnedRuntimeStop.self, payload: HvfOwnedRuntimeCodecTests.data("stop"))
        try ledger.admitStop(operationID: XCTUnwrap(UUID(uuidString: stop.operationID)))
    }
    func event(_ name: String, changing: (inout [String: Any]) -> Void) throws -> HvfOwnedRuntimeEvent {
        var value = try XCTUnwrap(JSONSerialization.jsonObject(with: HvfOwnedRuntimeCodecTests.data(name)) as? [String: Any])
        changing(&value)
        let data = try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys, .withoutEscapingSlashes])
        return try JSONDecoder().decode(HvfOwnedRuntimeEvent.self, from: data)
    }

    func testCompleteRequiresBothRoleFoldsAndRejectsEveryFollowingEvent() throws {
        var ledger = try ledger(); try admit(&ledger)
        for name in ["ready", "swtpm-started", "helper-started", "stop-ack", "helper-reaped", "swtpm-reaped", "complete"] {
            try ledger.accept(HvfOwnedRuntimeCodecTests.event(name))
        }
        XCTAssertNotNil(ledger.complete)
        XCTAssertThrowsError(try ledger.accept(HvfOwnedRuntimeCodecTests.event("complete")))
        XCTAssertThrowsError(try ledger.accept(event("ready") { $0["sequence"] = 8 }))
    }

    func testIdentitySequenceAndLifecycleRefusalDoNotAdvanceLedger() throws {
        var ledger = try ledger()
        for invalid in [try event("ready") { $0["runnerPID"] = 99 },
                        try event("ready") { $0["runToken"] = UUID().uuidString.lowercased() },
                        try event("ready") { $0["sequence"] = 2 },
                        try event("helper-started") { $0["sequence"] = 1 }] {
            XCTAssertThrowsError(try ledger.accept(invalid))
        }
        try ledger.accept(HvfOwnedRuntimeCodecTests.event("ready"))
        XCTAssertTrue(ledger.ready)
        XCTAssertThrowsError(try ledger.accept(event("helper-started") {
            $0["sequence"] = 2
            var child = $0["child"] as? [String: Any] ?? [:]; child["generation"] = 1; $0["child"] = child
        }))
        try ledger.accept(HvfOwnedRuntimeCodecTests.event("swtpm-started"))
        XCTAssertThrowsError(try ledger.accept(event("swtpm-started") { $0["sequence"] = 3 }))
    }

    func testEarlyCompletionAndEarlyAcknowledgementHaveDifferentAdmissionRules() throws {
        var early = try ledger()
        try early.accept(HvfOwnedRuntimeCodecTests.event("complete-not-admitted"))
        XCTAssertFalse(early.ready)
        var other = try ledger(); try admit(&other)
        try other.accept(event("stop-ack") { $0["sequence"] = 1 })
        XCTAssertTrue(other.stopAcknowledged)
        XCTAssertFalse(other.ready)
        XCTAssertThrowsError(try other.accept(event("stop-ack") { $0["sequence"] = 2 }))
        var unrequested = try ledger()
        XCTAssertThrowsError(try unrequested.accept(event("stop-ack") { $0["sequence"] = 1 }))
    }

    func testAcknowledgedStopCannotDisappearFromCompletionAndCauseMustMatchOutcome() throws {
        var ledger = try ledger(); try admit(&ledger)
        for name in ["ready", "swtpm-started", "helper-started", "stop-ack", "helper-reaped", "swtpm-reaped"] {
            try ledger.accept(HvfOwnedRuntimeCodecTests.event(name))
        }
        XCTAssertThrowsError(try ledger.accept(event("complete") {
            var complete = $0["complete"] as? [String: Any] ?? [:]
            complete["operationID"] = NSNull(); $0["complete"] = complete
        }))
        for (cause, outcome) in [("normalExit", "cancelled"), ("ownerEOF", "finished")] {
            let invalid = try event("complete") {
                var complete = $0["complete"] as? [String: Any] ?? [:]
                complete["cause"] = cause; complete["outcome"] = outcome; $0["complete"] = complete
            }
            XCTAssertThrowsError(try HvfOwnedRuntimeEventValidation.validate(invalid))
        }
        var early = try self.ledger()
        XCTAssertThrowsError(try early.accept(event("complete-not-admitted") {
            var complete = $0["complete"] as? [String: Any] ?? [:]
            complete["cause"] = "stopRequested"; $0["complete"] = complete
        }))
        try ledger.accept(HvfOwnedRuntimeCodecTests.event("complete"))
    }

    func testOutstandingChildCannotBeHiddenByAZeroSummary() throws {
        var ledger = try ledger()
        try ledger.accept(HvfOwnedRuntimeCodecTests.event("ready"))
        try ledger.accept(HvfOwnedRuntimeCodecTests.event("swtpm-started"))
        XCTAssertThrowsError(try ledger.accept(event("complete-not-admitted") {
            $0["sequence"] = 3
            var complete = $0["complete"] as? [String: Any] ?? [:]
            complete["mediaLeaseDisposition"] = "releasedAfterReap"; $0["complete"] = complete
        }))
        XCTAssertNil(ledger.complete)
    }
}
