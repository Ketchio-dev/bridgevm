import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfPointerScrollEncodingTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 2_000)

    func testAllSignedUIDeltasEncodeOneTwoEventRequest() {
        for ticks in -128...127 where ticks != 0 {
            let command = "scroll:\(ticks)@0x32767"
            guard let encoding = HvfGuestInputEncoding(.pointer(command)) else {
                return XCTFail("valid UI scroll rejected")
            }
            XCTAssertEqual(encoding.verb, "POINTERINPUT")
            XCTAssertEqual(encoding.insertedEventCount, 2)
            XCTAssertEqual(Data(base64Encoded: encoding.base64), Data(command.utf8))
        }
        XCTAssertNil(HvfGuestInputEncoding(.pointer("wheel:-128")))
        for command in ["scroll:0@0x0", "scroll:-129@0x0", "scroll:128@0x0", "scroll:1",
                        "scroll:1@", "scroll:1@0x0@0x0", "scroll:+1@0x0", "scroll:01@0x0",
                        "scroll:-0@0x0", "scroll:1@32768x0", "scroll:1@0x-1", "scroll:1@0x0\n"] {
            XCTAssertNil(HvfGuestInputEncoding(.pointer(command)), command)
        }
    }

    func testPartialScrollCancelsFollowingKeyWithoutReplay() {
        var stream = HvfAcknowledgedInputStream()
        XCTAssertTrue(stream.enqueue(.pointer("scroll:-128@32767x0"), now: now))
        XCTAssertTrue(stream.enqueue(.key("enter"), now: now))
        var command = ""
        _ = stream.advance(lines: [], now: now) { command = $0; return true }
        let fields = command.split(separator: " ")
        let label = fields.prefix(2).joined(separator: " ")
        let lines = ["BVAGENT CMD \(label) exit=0", "BVINPUT_INSERTED \(fields[1]) 1", "BVAGENT END \(label)"]
        XCTAssertEqual(stream.advance(lines: lines, now: now) { _ in false },
                       .cancelled(.transportFailed, discarded: 2))
        XCTAssertEqual(stream.advance(lines: [], now: now) { _ in XCTFail("scroll replay"); return true }, .idle)
    }
}
