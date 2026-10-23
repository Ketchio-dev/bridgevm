import Foundation
import XCTest
@testable import BridgeVMControl

/// While a request waits for its receipt, a newer absolute move replaces the newest unsent move of the
/// same generation in place. Nothing else is merged, reordered or rewritten, and only sent requests
/// are awaited.
final class HvfOrderedPointerCoalescingTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 10_000)

    func testDragSendsThePressTheNewestMoveAndTheRelease() {
        var fixture = HvfPointerFailureFixture()
        fixture.route([.pointer("press:10x10"), .pointer("move:20x20"), .pointer("move:30x30"),
                       .pointer("move:40x40"), .pointer("releaseall:40x40")])
        XCTAssertEqual(fixture.payloads(), ["press:10x10"], "only the press is in flight")
        XCTAssertEqual(fixture.router.count, 3)
        XCTAssertEqual(fixture.acknowledgeAll(), ["press:10x10", "move:40x40", "releaseall:40x40"])
    }

    func testMovesAreNeverMergedAcrossAnyOtherEvent() {
        var fixture = HvfPointerFailureFixture()
        let separators: [HvfOrderedInputQueue.Event] = [
            .pointer("press:2x2"), .pointer("releaseall:3x3"), .pointer("click:4x4"), .pointer("rightclick:5x5"),
            .pointer("rightpress:6x6"), .pointer("scroll:-1@7x7"), .pointer("wheel:1"), .key("enter"), .text("a"),
        ]
        fixture.route([.pointer("move:1x1")])
        var expected = ["move:1x1"]
        for (index, separator) in separators.enumerated() {
            fixture.route([.pointer("move:\(index)x10"), .pointer("move:\(index)x20"), separator])
            switch separator {
            case let .pointer(value), let .key(value), let .text(value): expected += ["move:\(index)x20", value]
            }
        }
        fixture.route([.pointer("move:99x10"), .pointer("move:99x20")])
        XCTAssertEqual(fixture.acknowledgeAll(), expected + ["move:99x20"])
    }

    func testAnInFlightMoveIsNeverRewritten() throws {
        var fixture = HvfPointerFailureFixture()
        fixture.route([.pointer("move:1x1"), .pointer("move:2x2")])
        XCTAssertEqual(fixture.router.count, 2, "a move behind an in-flight move is queued, not merged into it")
        fixture.route([.pointer("move:3x3")])
        XCTAssertEqual(fixture.router.count, 2)
        XCTAssertEqual(fixture.payloads(), ["move:1x1"])
        let inFlight = try XCTUnwrap(fixture.commands.first)
        let id = try XCTUnwrap(UUID(uuidString: String(inFlight.split(separator: " ")[1])))
        XCTAssertEqual(fixture.poll(lines: fixture.receipt(inFlight)), .inserted(id), "its own receipt completes it")
        fixture.poll()
        XCTAssertEqual(fixture.payloads(), ["move:1x1", "move:3x3"])
    }

    func testACoalescedMoveIsNeverAwaited() {
        var fixture = HvfPointerFailureFixture()
        fixture.route([.pointer("press:1x1"), .pointer("move:2x2"), .pointer("move:3x3")])
        XCTAssertEqual(fixture.acknowledgeAll(), ["press:1x1", "move:3x3"])
        fixture.now = fixture.now.addingTimeInterval(31)
        XCTAssertEqual(fixture.poll(), .idle, "no receipt is awaited for move:2x2, so nothing expires")
        XCTAssertFalse(fixture.router.failed)
        XCTAssertEqual(fixture.router.route(.key("enter"), binding: fixture.binding, now: fixture.now), .queued)
    }

    func testPointerCleanupFollowsTheEventsThatWereSent() {
        var rejected = HvfPointerFailureFixture()
        rejected.route([.pointer("press:1x1"), .pointer("move:2x2"), .pointer("move:3x3")])
        rejected.poll(lines: rejected.receipt(rejected.commands[0]))
        rejected.poll()
        XCTAssertEqual(rejected.payloads(), ["press:1x1", "move:3x3"])
        rejected.poll(lines: rejected.receipt(rejected.commands[rejected.commands.count - 1], inserted: nil))
        XCTAssertEqual(rejected.router.failure, .requestFailed)
        rejected.poll()
        XCTAssertEqual(rejected.payloads().last, "releaseall:3x3", "the cleanup release uses the sent position")

        var completed = HvfPointerFailureFixture()
        completed.route([.pointer("press:1x1"), .pointer("move:2x2"), .pointer("move:3x3"), .pointer("releaseall:3x3")])
        XCTAssertEqual(completed.acknowledgeAll(), ["press:1x1", "move:3x3", "releaseall:3x3"])
        completed.route([.text("fail")])
        let sent = completed.commands.count
        completed.poll(lines: completed.receipt(completed.commands[sent - 1], inserted: nil))
        XCTAssertEqual(completed.router.failure, .requestFailed)
        completed.poll()
        XCTAssertEqual(completed.commands.count, sent, "a completed drag leaves no button to release")
        XCTAssertEqual(completed.router.count, 0)
    }

    func testAMoveForAnotherBindingIsRefusedAndMergesNothing() {
        var fixture = HvfPointerFailureFixture()
        fixture.route([.pointer("press:1x1"), .pointer("move:2x2")])
        XCTAssertEqual(fixture.router.route(.pointer("move:3x3"), binding: ["other"], now: fixture.now), .refused)
        XCTAssertEqual(fixture.router.failure, .bindingChanged)
        XCTAssertEqual(fixture.router.count, 0)
        XCTAssertEqual(fixture.payloads(), ["press:1x1"])
    }

    func testQueueReplacesOnlyTheNewestUnsentMoveWhileARequestIsInFlight() throws {
        var idle = HvfOrderedInputQueue()
        XCTAssertTrue(idle.enqueue(.pointer("move:1x1"), now: now))
        XCTAssertTrue(idle.enqueue(.pointer("move:2x2"), now: now))
        XCTAssertEqual(idle.count, 2, "with nothing in flight the next poll sends the older move at once")

        var queue = HvfOrderedInputQueue()
        XCTAssertTrue(queue.enqueue(.pointer("press:1x1"), now: now))
        let press = try sent(queue.poll(now: now))
        for _ in 0..<62 { XCTAssertTrue(queue.enqueue(.key("a"), now: now)) }
        XCTAssertTrue(queue.enqueue(.pointer("move:1x1"), now: now))
        XCTAssertEqual(queue.count, HvfOrderedInputQueue.maximumEvents)
        XCTAssertFalse(queue.enqueue(.key("b"), now: now))
        XCTAssertTrue(queue.enqueue(.pointer("move:32767x32767"), now: now), "a replacement needs no free slot")
        XCTAssertEqual(queue.count, HvfOrderedInputQueue.maximumEvents)
        let others = "press:1x1".utf8.count + 62
        XCTAssertEqual(queue.byteCount, others + "move:32767x32767".utf8.count)
        XCTAssertTrue(queue.enqueue(.pointer("move:5x5"), now: now))
        XCTAssertEqual(queue.byteCount, others + "move:5x5".utf8.count)
        XCTAssertEqual(queue.acknowledge(press, succeeded: true, now: now), .completed)
        for _ in 0..<62 {
            let key = try sent(queue.poll(now: now))
            XCTAssertEqual(queue.acknowledge(key, succeeded: true, now: now), .completed)
        }
        let move = try sent(queue.poll(now: now))
        XCTAssertEqual(move.event, .pointer("move:5x5"))
        XCTAssertEqual(queue.acknowledge(move, succeeded: true, now: now), .completed)
        XCTAssertEqual(queue.poll(now: now), .idle)
        XCTAssertEqual(queue.byteCount, 0)
    }

    func testAReplacementOverTheByteLimitIsRefusedAndKeepsTheQueuedMove() throws {
        var queue = HvfOrderedInputQueue()
        let text = String(repeating: "a", count: HvfOrderedInputQueue.maximumBytes - 15)
        XCTAssertTrue(queue.enqueue(.text(text), now: now))
        let active = try sent(queue.poll(now: now))
        XCTAssertTrue(queue.enqueue(.pointer("move:1x1"), now: now))
        XCTAssertFalse(queue.enqueue(.pointer("move:32767x32767"), now: now))
        XCTAssertEqual(queue.byteCount, HvfOrderedInputQueue.maximumBytes - 7)
        XCTAssertTrue(queue.enqueue(.pointer("move:2x2"), now: now))
        XCTAssertEqual(queue.count, 2)
        XCTAssertEqual(queue.acknowledge(active, succeeded: true, now: now), .completed)
        XCTAssertEqual(try sent(queue.poll(now: now)).event, .pointer("move:2x2"))
    }

    private func sent(_ transition: HvfOrderedInputQueue.Transition) throws -> HvfOrderedInputQueue.Ticket {
        guard case let .send(ticket) = transition else {
            XCTFail("Expected a dispatched input ticket, got \(transition)")
            throw NSError(domain: "HvfOrderedPointerCoalescingTests", code: 1)
        }
        return ticket
    }
}

private extension HvfPointerFailureFixture {
    /// Routes events as the display does: every queued route polls at once (`HvfSessionInputDriver`).
    mutating func route(_ events: [HvfOrderedInputQueue.Event]) {
        for event in events {
            XCTAssertEqual(router.route(event, binding: binding, now: now), .queued, "\(event)")
            poll()
        }
    }

    /// Answers each sent request with its exact receipt, then polls without lines as the driver does,
    /// until nothing is held. Returns every payload in send order.
    mutating func acknowledgeAll() -> [String] {
        var answered = 0
        while answered < commands.count {
            let command = commands[answered]
            answered += 1
            guard case .inserted = poll(lines: receipt(command, inserted: insertedEvents(command))) else {
                XCTFail("no insertion receipt accepted for \(command)")
                break
            }
            poll()
        }
        XCTAssertEqual(router.count, 0, "nothing left to send or await")
        return payloads()
    }

    func insertedEvents(_ command: String) -> Int? {
        let fields = command.split(separator: " ")
        guard fields.count == 3, let bytes = Data(base64Encoded: String(fields[2])) else { return nil }
        let value = String(decoding: bytes, as: UTF8.self)
        switch fields[0] {
        case "TEXTINPUT": return HvfGuestInputEncoding(.text(value))?.insertedEventCount
        case "KEYINPUT": return HvfGuestInputEncoding(.key(value))?.insertedEventCount
        default: return HvfGuestInputEncoding(.pointer(value))?.insertedEventCount
        }
    }
}
