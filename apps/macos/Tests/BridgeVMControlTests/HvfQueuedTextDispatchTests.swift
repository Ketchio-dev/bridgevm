import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfQueuedTextDispatchTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 8_000)
    private let binding = ["owned", "run"]

    private func ready(_ router: inout HvfSessionInputRouter) {
        router.beginOwnedBoot(binding: binding)
        var command = ""
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { command = $0; return true }
        let id = command.split(separator: " ")[1]
        _ = router.poll(binding: binding, serviceReady: true, lines: ["BVAGENT CMD \(command) exit=0",
            "BVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536", "BVINPUT_TEXT \(id) QUEUED_DISPATCH",
            "BVAGENT END \(command)"], now: now) { _ in false }
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { _ in false }
    }

    private func reply(_ command: String, marker: String, exit: Int = 0) -> [String] {
        let label = command.split(separator: " ").prefix(2).joined(separator: " ")
        return ["BVAGENT CMD \(label) exit=\(exit)", marker, "BVAGENT END \(label)"]
    }

    func testQueuedDispatchPreservesWholeTextAndEnterOrdering() {
        var router = HvfSessionInputRouter(); ready(&router)
        XCTAssertEqual(router.route(.text("한🙂"), binding: binding, now: now), .queued)
        XCTAssertEqual(router.route(.key("enter"), binding: binding, now: now), .queued)
        var commands: [String] = []
        let send: (String) -> Bool = { commands.append($0); return true }
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now, send: send)
        XCTAssertTrue(commands[0].hasSuffix(" S"))
        let id = commands[0].split(separator: " ")[1]
        let done = router.poll(binding: binding, serviceReady: true,
            lines: reply(commands[0], marker: "BVTEXT_DISPATCHED \(id) 3"), now: now, send: send)
        guard case .inserted = done else { return XCTFail("Dispatch did not complete") }
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now, send: send)
        XCTAssertEqual(commands.count, 2)
        XCTAssertTrue(commands[1].hasPrefix("KEYINPUT "))
        XCTAssertFalse(commands[1].hasSuffix(" S"))
        XCTAssertFalse(router.failed)
    }

    func testExplicitUnsupportedFallsBackWithoutAdvancingTheRecoveryLedger() {
        var router = HvfSessionInputRouter(); ready(&router)
        _ = router.route(.text("한🙂"), binding: binding, now: now)
        _ = router.route(.key("enter"), binding: binding, now: now)
        var commands: [String] = []
        let send: (String) -> Bool = { commands.append($0); return true }
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now, send: send)
        let first = commands[0], firstID = first.split(separator: " ")[1]
        _ = router.poll(binding: binding, serviceReady: true,
            lines: reply(first, marker: "BVTEXT_UNSUPPORTED \(firstID)"), now: now, send: send)
        XCTAssertEqual(router.count, 2)
        XCTAssertEqual(commands.count, 2)
        let fallback = commands[1], fallbackID = fallback.split(separator: " ")[1]
        XCTAssertNotEqual(firstID, fallbackID)
        XCTAssertFalse(fallback.hasSuffix(" S"))
        _ = router.poll(binding: binding, serviceReady: true,
            lines: reply(fallback, marker: "BVINPUT_INSERTED \(fallbackID) 6"), now: now, send: send)
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now, send: send)
        XCTAssertEqual(commands.count, 3)
        XCTAssertTrue(commands[2].hasPrefix("KEYINPUT "))
        XCTAssertEqual(router.count, 1)
        XCTAssertFalse(router.failed)
    }

    func testUnknownDispatchNeverFallsBackAndWrongUnitCountIsRejected() {
        for unknown in [true, false] {
            var stream = HvfAcknowledgedInputStream()
            XCTAssertTrue(stream.enqueue(.text("한🙂"), now: now))
            var commands: [String] = []
            let send: (String) -> Bool = { commands.append($0); return true }
            _ = stream.advance(lines: [], now: now, queuedTextDispatch: true, send: send)
            let command = commands[0], id = command.split(separator: " ")[1]
            let marker = unknown ? "BVINPUT_FAILED \(id) unicode-packet-outcome-unknown dispatched_units=1"
                                 : "BVTEXT_DISPATCHED \(id) 2"
            XCTAssertEqual(stream.advance(lines: reply(command, marker: marker, exit: unknown ? 1 : 0), now: now,
                queuedTextDispatch: true, send: send), .cancelled(.transportFailed, discarded: 1))
            XCTAssertEqual(commands.count, 1)
        }
    }
}
