import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfPacedUnicodeStreamTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 2_000)
    private func receipt(_ command: String, count: Int) -> [String] {
        let id = command.split(separator: " ")[1]
        return ["BVAGENT CMD \(command) exit=0", "BVINPUT_INSERTED \(id) \(count)", "BVAGENT END \(command)"]
    }
    private func text(_ command: String) -> String? {
        let fields = command.split(separator: " ")
        guard fields.count == 3, let data = Data(base64Encoded: String(fields[2])) else { return nil }
        return String(data: data, encoding: .utf8)
    }
    func testTextWaitsForEachScalarReceiptBeforeSendingTheNextAndEnter() {
        var stream = HvfAcknowledgedInputStream()
        var time = now
        XCTAssertTrue(stream.enqueue(.text("한🙂글"), now: time))
        XCTAssertTrue(stream.enqueue(.key("enter"), now: time))
        var commands: [String] = []
        let send: (String) -> Bool = { commands.append($0); return true }
        _ = stream.advance(lines: [], now: time, send: send)
        XCTAssertEqual(text(commands[0]), "한")
        for (index, expected) in ["한", "🙂", "글"].enumerated() {
            guard commands.count == index + 1 else { return XCTFail("Missing scalar command") }
            XCTAssertEqual(text(commands[index]), expected)
            XCTAssertEqual(stream.advance(lines: [], now: time, send: send), .waiting)
            let completed = stream.advance(lines: receipt(commands[index], count: expected.utf16.count * 2), now: time, send: send)
            if index < 2 {
                XCTAssertEqual(completed, .waiting)
                XCTAssertEqual(stream.count, 2)
                XCTAssertEqual(stream.advance(lines: [], now: time + 0.049) { _ in
                    XCTFail("Continued before the text pacing interval"); return true
                }, .waiting)
            } else {
                guard case .inserted = completed else { return XCTFail("Whole event not completed") }
            }
            time += 0.05
            _ = stream.advance(lines: [], now: time, send: send)
        }
        XCTAssertEqual(commands.count, 4)
        XCTAssertTrue(commands.last?.hasPrefix("KEYINPUT ") == true)
        XCTAssertEqual(Set(commands.map { $0.split(separator: " ")[1] }).count, 4)
    }
    func testZeroInsertionAfterAPrefixNeverReplaysTheWholeText() {
        var stream = HvfAcknowledgedInputStream()
        XCTAssertTrue(stream.enqueue(.text("ab"), now: now))
        XCTAssertTrue(stream.enqueue(.key("enter"), now: now))
        var command = ""
        let send: (String) -> Bool = { command = $0; return true }
        _ = stream.advance(lines: [], now: now, send: send)
        _ = stream.advance(lines: receipt(command, count: 2), now: now, send: send)
        _ = stream.advance(lines: [], now: now + 0.05, send: send)
        let id = command.split(separator: " ")[1]
        let failed = ["BVAGENT CMD \(command) exit=1", "BVINPUT_FAILED \(id) unicode-input-incomplete inserted=0 requested=2"]
        XCTAssertEqual(stream.advance(lines: failed, now: now + 0.05, send: send), .cancelled(.transportFailed, discarded: 2))
        XCTAssertEqual(stream.count, 0)
    }
    func testLargeCommitKeepsOneQueueSlotAndOriginalDeadline() {
        var stream = HvfAcknowledgedInputStream()
        XCTAssertTrue(stream.enqueue(.text(String(repeating: "가", count: 100)), now: now))
        XCTAssertEqual(stream.count, 1)
        var command = ""
        _ = stream.advance(lines: [], now: now) { command = $0; return true }
        XCTAssertEqual(text(command), "가")
        XCTAssertEqual(stream.advance(lines: receipt(command, count: 2), now: now.addingTimeInterval(30)) { _ in
            XCTFail("Expired prefix was continued"); return true
        }, .cancelled(.expired, discarded: 1))
    }
}
