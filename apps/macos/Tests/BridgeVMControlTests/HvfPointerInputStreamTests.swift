import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfPointerInputStreamTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 2_000)

    private func receipt(_ command: String, count: Int) -> [String] {
        let fields = command.split(separator: " ")
        let label = fields.prefix(2).joined(separator: " ")
        return ["BVAGENT CMD \(label) exit=0", "BVINPUT_INSERTED \(fields[1]) \(count)", "BVAGENT END \(label)"]
    }

    func testPointerGrammarMatchesGuestBoundsAndCounts() {
        for coordinate in 0...32767 {
            XCTAssertEqual(HvfPointerInputEncoding.eventCount("move:\(coordinate)x\(coordinate)"), 1)
        }
        for verb in ["press", "release", "rightpress", "rightrelease"] {
            XCTAssertEqual(HvfPointerInputEncoding.eventCount("\(verb):0x32767"), 1)
        }
        for verb in ["click", "rightclick"] {
            XCTAssertEqual(HvfPointerInputEncoding.eventCount("\(verb):32767x0"), 2)
        }
        for ticks in [-127, -1, 1, 127] {
            XCTAssertEqual(HvfPointerInputEncoding.eventCount("wheel:\(ticks)"), 1)
        }
        var stream = HvfAcknowledgedInputStream()
        for command in ["", "click", "click:0x0:1", "move:32768x0", "move:0x32768",
                        "move:-1x0", "move:01x0", "move:+1x0", "move:0x", "move:0x0\n",
                        "wheel:0", "wheel:-0", "wheel:128", "wheel:-128", "wheel:+1", "wheel:01",
                        "wheel:1 ", "wheel:999999999999999999999999", "drag:0x0"] {
            XCTAssertFalse(stream.enqueue(.pointer(command), now: now), command)
        }
        XCTAssertEqual(stream.count, 0)
    }

    func testClickTextAndKeyWaitForTheirOwnInsertionReceipts() {
        var stream = HvfAcknowledgedInputStream()
        let events: [HvfOrderedInputQueue.Event] = [.pointer("click:0x32767"), .text("a"), .key("enter")]
        for event in events { XCTAssertTrue(stream.enqueue(event, now: now)) }
        for verb in ["POINTERINPUT", "TEXTINPUT", "KEYINPUT"] {
            var command = ""
            guard case let .sent(id) = stream.advance(lines: [], now: now, send: { command = $0; return true }) else {
                return XCTFail("request not sent")
            }
            XCTAssertTrue(command.hasPrefix(verb + " "))
            XCTAssertEqual(stream.advance(lines: [], now: now) { _ in XCTFail("duplicate send"); return true }, .waiting)
            XCTAssertEqual(stream.advance(lines: receipt(command, count: 2), now: now) {
                _ in XCTFail("next input sent in receipt batch"); return true
            }, .inserted(id))
        }
        XCTAssertEqual(stream.count, 0)
    }

    func testPartialClickCancelsFollowingTextWithoutReplay() {
        var stream = HvfAcknowledgedInputStream()
        XCTAssertTrue(stream.enqueue(.pointer("click:1x2"), now: now))
        XCTAssertTrue(stream.enqueue(.text("must not send"), now: now))
        var command = ""
        _ = stream.advance(lines: [], now: now) { command = $0; return true }
        XCTAssertEqual(stream.advance(lines: receipt(command, count: 1), now: now) { _ in false },
                       .cancelled(.transportFailed, discarded: 2))
        XCTAssertEqual(stream.advance(lines: receipt(command, count: 2), now: now) {
            _ in XCTFail("replay after unknown insertion prefix"); return true
        }, .idle)
    }
}
