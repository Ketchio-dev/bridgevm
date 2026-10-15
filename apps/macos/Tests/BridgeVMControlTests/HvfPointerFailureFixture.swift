import Foundation
import XCTest
@testable import BridgeVMControl

struct HvfPointerFailureFixture {
    let binding = ["disk", "vars", "evidence", "control"]
    var now = Date(timeIntervalSince1970: 2_000)
    var router = HvfSessionInputRouter()
    var commands: [String] = []

    init() {
        router.beginOwnedBoot(binding: binding)
        _ = poll()
        let command = commands.removeLast(), id = command.split(separator: " ")[1]
        _ = poll(lines: ["BVAGENT CMD \(command) exit=0",
            "BVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536", "BVAGENT END \(command)"])
        _ = poll()
        XCTAssertEqual(router.state, .ready)
    }

    @discardableResult
    mutating func poll(lines: [String] = [], ready: Bool = true, binding next: [String]? = nil,
                       send: Bool = true) -> HvfAcknowledgedInputStream.Update? {
        var sent: [String] = []
        let update = router.poll(binding: next ?? binding, serviceReady: ready, lines: lines, now: now) {
            sent.append($0); return send
        }
        commands += sent
        return update
    }

    @discardableResult
    mutating func send(_ event: HvfOrderedInputQueue.Event) -> String {
        XCTAssertEqual(router.route(event, binding: binding, now: now), .queued)
        _ = poll()
        return commands.last ?? "missing"
    }

    mutating func failAfterConfirmedPress() {
        let press = send(.pointer("press:1x2"))
        _ = poll(lines: receipt(press))
        let text = send(.text("fail"))
        _ = poll(lines: receipt(text, inserted: nil))
        XCTAssertEqual(router.failure, .requestFailed)
    }

    func receipt(_ command: String, inserted: Int? = 1) -> [String] {
        let fields = command.split(separator: " "), label = fields.prefix(2).joined(separator: " ")
        return ["BVAGENT CMD \(label) exit=\(inserted == nil ? 1 : 0)"]
            + (inserted.map { ["BVINPUT_INSERTED \(fields[1]) \($0)"] } ?? [])
            + ["BVAGENT END \(label)"]
    }

    func payloads() -> [String] {
        commands.map { command in
            let fields = command.split(separator: " ")
            guard fields.count == 3, let bytes = Data(base64Encoded: String(fields[2])) else { return "invalid" }
            return String(decoding: bytes, as: UTF8.self)
        }
    }

    mutating func assertFailedClosed() {
        XCTAssertTrue(router.failed)
        XCTAssertEqual(router.state, .failed)
        XCTAssertEqual(router.route(.text("refused"), binding: binding, now: now), .refused)
        XCTAssertFalse(router.allowLegacyWrite(binding: binding))
    }
}
