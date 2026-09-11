import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfGuestKeyInputRequestTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_000)
    private let id = UUID(uuidString: "12345678-1234-1234-1234-123456789abc")!

    func testSupportedKeyChordsProduceExactWirePayloadAndEventCount() throws {
        let navigation = ["left", "up", "right", "down", "home", "end", "pageup", "pagedown"]
        let named = navigation + ["backspace", "tab", "enter", "esc", "escape", "space", "insert", "delete"]
            + (1...12).map { "f\($0)" }
        let shifted = navigation.map { "shift+" + $0 } + ["shift+tab"]
        let controlled = (navigation + ["a", "c", "v", "x", "y", "z", "backspace", "delete"]).map { "ctrl+" + $0 }
        let combined = navigation.map { "ctrl+shift+" + $0 }
        for key in named + shifted + controlled + combined + ["alt+tab", "alt+enter", "alt+f4"] {
            let request = try XCTUnwrap(HvfUnicodeInputRequest(event: .key(key), now: now, id: id))
            XCTAssertEqual(request.command, "KEYINPUT \(id.uuidString) \(Data(key.utf8).base64EncodedString())")
            XCTAssertEqual(request.insertedEventCount, key.split(separator: "+").count * 2)
        }
    }

    func testUnsupportedCommandsNeverBecomeRequests() {
        for key in ["", "a", "CTRL+V", "ctrl+alt+delete", "command+c", "shift+ctrl+left",
                    "ctrl+ctrl+v", "+enter", "enter+", "ctrl++v", "alt+a", "shift+enter",
                    "enter\n", "enter\r", " ctrl+v", "f13", String(repeating: "a", count: 33)] {
            XCTAssertNil(HvfUnicodeInputRequest(event: .key(key), now: now), key)
        }
    }

    func testKeyReceiptRequiresExactCountAndEnd() throws {
        var request = try XCTUnwrap(HvfUnicodeInputRequest(event: .key("ctrl+shift+left"), now: now, id: id))
        XCTAssertNil(request.consume(lines: ["BVAGENT CMD \(request.command) exit=0"], now: now))
        XCTAssertNil(request.consume(lines: ["BVINPUT_INSERTED \(id.uuidString) 6"], now: now))
        XCTAssertEqual(request.consume(lines: ["BVAGENT END \(request.command)"], now: now), .inserted)
        XCTAssertNil(request.consume(lines: ["BVAGENT END \(request.command)"], now: now))
    }

    func testPartialKeyReceiptFailsWithoutReplay() throws {
        var request = try XCTUnwrap(HvfUnicodeInputRequest(event: .key("ctrl+v"), now: now, id: id))
        let lines = ["BVAGENT CMD \(request.command) exit=0",
                     "BVINPUT_INSERTED \(id.uuidString) 1", "BVAGENT END \(request.command)"]
        XCTAssertEqual(request.consume(lines: lines, now: now), .failed(.invalidReceipt))
        XCTAssertNil(request.consume(lines: lines, now: now))
    }

    func testExistingTextInitializerMatchesQueueEventEncoding() throws {
        let text = "\u{D55C}\u{1F600}"
        let legacy = try XCTUnwrap(HvfUnicodeInputRequest(text: text, now: now, id: id))
        let queued = try XCTUnwrap(HvfUnicodeInputRequest(event: .text(text), now: now, id: id))
        XCTAssertEqual(legacy.command, queued.command)
        XCTAssertEqual(queued.insertedEventCount, 6)
        XCTAssertNil(HvfUnicodeInputRequest(event: .text(""), now: now))
        XCTAssertNil(HvfUnicodeInputRequest(event: .text(String(repeating: "a", count: 65_537)), now: now))
    }
}
