import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfOrderedInputQueueTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 10_000)

    func testCompositionCommitsAndEditingKeyWaitForAcknowledgements() throws {
        var queue = HvfOrderedInputQueue()
        XCTAssertTrue(queue.enqueue(.text("\u{d55c}"), now: now))
        XCTAssertTrue(queue.enqueue(.text("\u{ae00}"), now: now))
        XCTAssertTrue(queue.enqueue(.key("enter"), now: now))
        let first = try ticket(queue.poll(now: now))
        XCTAssertEqual(first.event, .text("\u{d55c}"))
        XCTAssertEqual(queue.poll(now: now), .waiting)
        XCTAssertEqual(queue.acknowledge(first, succeeded: true, now: now), .completed)
        let second = try ticket(queue.poll(now: now))
        XCTAssertEqual(second.event, .text("\u{ae00}"))
        XCTAssertEqual(queue.acknowledge(first, succeeded: true, now: now), .ignored)
        XCTAssertEqual(queue.poll(now: now), .waiting)
        XCTAssertEqual(queue.acknowledge(second, succeeded: true, now: now), .completed)
        let last = try ticket(queue.poll(now: now))
        XCTAssertEqual(last.event, .key("enter"))
        XCTAssertEqual(queue.acknowledge(last, succeeded: true, now: now), .completed)
        XCTAssertEqual(queue.poll(now: now), .idle)
        XCTAssertEqual(queue.byteCount, 0)
    }

    func testTransportFailureCancelsDependentInputWithoutSendingIt() throws {
        var queue = HvfOrderedInputQueue()
        XCTAssertTrue(queue.enqueue(.text("\u{d55c}"), now: now))
        XCTAssertTrue(queue.enqueue(.key("enter"), now: now))
        let first = try ticket(queue.poll(now: now))
        XCTAssertEqual(queue.acknowledge(first, succeeded: false, now: now),
                       .cancelled(.transportFailed, discarded: 2))
        XCTAssertEqual(queue.poll(now: now), .idle)
        XCTAssertEqual(queue.count, 0)
        XCTAssertEqual(queue.byteCount, 0)
    }

    func testResetRejectsLateReceiptsEvenForIdenticalNewText() throws {
        var queue = HvfOrderedInputQueue()
        XCTAssertTrue(queue.enqueue(.text("same"), now: now))
        let old = try ticket(queue.poll(now: now))
        XCTAssertEqual(queue.cancel(.sessionChanged), .cancelled(.sessionChanged, discarded: 1))
        XCTAssertTrue(queue.enqueue(.text("same"), now: now))
        let current = try ticket(queue.poll(now: now))
        XCTAssertNotEqual(old.generation, current.generation)
        XCTAssertEqual(queue.acknowledge(old, succeeded: true, now: now), .ignored)
        XCTAssertEqual(queue.poll(now: now), .waiting)
        XCTAssertEqual(queue.cancel(.targetChanged), .cancelled(.targetChanged, discarded: 1))
        XCTAssertEqual(queue.acknowledge(current, succeeded: true, now: now), .ignored)
    }

    func testQueuedAndActiveTimeoutsNeverReplayUnknownInput() throws {
        var queue = HvfOrderedInputQueue()
        XCTAssertTrue(queue.enqueue(.text("queued"), now: now))
        XCTAssertEqual(queue.poll(now: now.addingTimeInterval(30)), .cancelled(.expired, discarded: 1))
        XCTAssertTrue(queue.enqueue(.text("active"), now: now))
        let active = try ticket(queue.poll(now: now))
        XCTAssertTrue(queue.enqueue(.key("enter"), now: now))
        XCTAssertEqual(queue.acknowledge(active, succeeded: true, now: now.addingTimeInterval(30)),
                       .cancelled(.expired, discarded: 2))
        XCTAssertEqual(queue.poll(now: now.addingTimeInterval(31)), .idle)
        XCTAssertTrue(queue.enqueue(.text("waiting"), now: now))
        _ = try ticket(queue.poll(now: now))
        XCTAssertEqual(queue.poll(now: now.addingTimeInterval(30)), .cancelled(.expired, discarded: 1))
    }

    func testCountAndUTF8ByteLimitsIncludeActiveInputAndRejectWithoutEviction() throws {
        var queue = HvfOrderedInputQueue()
        XCTAssertFalse(queue.enqueue(.text(""), now: now))
        let full = String(repeating: "\u{d55c}", count: 21_845) + "x"
        XCTAssertTrue(queue.enqueue(.text(full), now: now))
        let active = try ticket(queue.poll(now: now))
        XCTAssertEqual(queue.byteCount, HvfOrderedInputQueue.maximumBytes)
        XCTAssertFalse(queue.enqueue(.key("a"), now: now))
        XCTAssertEqual(queue.acknowledge(active, succeeded: true, now: now), .completed)
        for _ in 0..<HvfOrderedInputQueue.maximumEvents {
            XCTAssertTrue(queue.enqueue(.key("a"), now: now))
        }
        _ = try ticket(queue.poll(now: now))
        XCTAssertFalse(queue.enqueue(.key("b"), now: now))
        XCTAssertEqual(queue.count, HvfOrderedInputQueue.maximumEvents)
    }

    private func ticket(_ transition: HvfOrderedInputQueue.Transition) throws -> HvfOrderedInputQueue.Ticket {
        guard case .send(let ticket) = transition else {
            XCTFail("Expected a dispatched input ticket")
            throw NSError(domain: "HvfOrderedInputQueueTests", code: 1)
        }
        return ticket
    }
}
