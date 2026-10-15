import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfShiftKeyCompatibilityTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 4_000)
    private let binding = ["disk", "vars", "evidence", "control"]

    func testOldGuestRefusesNewChordsWithoutDisturbingQueuedOrLaterText() throws {
        var router = HvfSessionInputRouter()
        router.beginOwnedBoot(binding: binding)
        negotiate(&router, expanded: false)
        XCTAssertEqual(router.route(.text("before"), binding: binding, now: now), .queued)
        for key in ["shift+enter", "shift+space"] {
            XCTAssertEqual(router.route(.key(key), binding: binding, now: now), .refused)
            XCTAssertEqual(router.count, 1)
            XCTAssertEqual(router.state, .ready)
            XCTAssertFalse(router.failed)
        }
        XCTAssertEqual(router.route(.text("after"), binding: binding, now: now), .queued)
        for text in ["before", "after"] {
            var command = ""
            _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { command = $0; return true }
            XCTAssertTrue(command.hasPrefix("TEXTINPUT "))
            XCTAssertEqual(command.split(separator: " ").last.map(String.init), Data(text.utf8).base64EncodedString())
            let label = command.split(separator: " ").prefix(2).joined(separator: " ")
            let id = try XCTUnwrap(command.split(separator: " ").dropFirst().first)
            let lines = ["BVAGENT CMD \(label) exit=0", "BVINPUT_INSERTED \(id) \(text.utf16.count * 2)", "BVAGENT END \(label)"]
            _ = router.poll(binding: binding, serviceReady: true, lines: lines, now: now) { _ in XCTFail("receipt replay"); return false }
        }
        XCTAssertEqual(router.count, 0)
        XCTAssertEqual(router.state, .ready)
        XCTAssertFalse(router.failed)
    }

    func testExpandedGuestDispatchesShiftChordsAndTargetChangeKeepsSupport() throws {
        var router = HvfSessionInputRouter()
        router.beginOwnedBoot(binding: binding)
        negotiate(&router, expanded: true)
        for key in ["shift+enter", "shift+space"] {
            _ = router.cancelTarget(now: now)
            XCTAssertEqual(router.route(.key(key), binding: binding, now: now), .queued)
            var command = ""
            _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { command = $0; return true }
            XCTAssertTrue(command.hasPrefix("KEYINPUT "))
            XCTAssertEqual(command.split(separator: " ").last.map(String.init), Data(key.utf8).base64EncodedString())
        }
    }

    func testFreshOwnedBootDoesNotInheritExpandedKeySupport() {
        var router = HvfSessionInputRouter()
        router.beginOwnedBoot(binding: binding)
        negotiate(&router, expanded: true)
        router.beginOwnedBoot(binding: binding)
        negotiate(&router, expanded: false)
        _ = router.cancelTarget(now: now)
        XCTAssertEqual(router.route(.key("shift+enter"), binding: binding, now: now), .refused)
        XCTAssertEqual(router.route(.text("still available"), binding: binding, now: now), .queued)
    }

    func testOldGuestDisplayChordSignalsRefusalAndSubsequentTextRemainsAvailable() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session)
            let before = try fixture.commands()
            var refusals = 0
            for character in ["\r", " "] {
                let event = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero,
                    modifierFlags: .shift, timestamp: 0, windowNumber: 0, context: nil,
                    characters: character, charactersIgnoringModifiers: character, isARepeat: false, keyCode: 0))
                XCTAssertTrue(HvfFramebufferKeyInput.send(event, to: session, refused: { refusals += 1 }))
            }
            XCTAssertEqual(refusals, 2)
            XCTAssertEqual(try fixture.commands(), before)
            XCTAssertNil(try fixture.bytes(fixture.input), "No cross-transport fallback")
            XCTAssertTrue(session.sendDisplayInput(.text("ordinary")))
            XCTAssertTrue(try XCTUnwrap(fixture.commands().last).hasPrefix("TEXTINPUT "))
        }
    }

    private func negotiate(_ router: inout HvfSessionInputRouter, expanded: Bool) {
        var command = ""
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { command = $0; return true }
        let id = command.split(separator: " ").last ?? "missing"
        var lines = ["BVAGENT CMD \(command) exit=0", "BVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536"]
        if expanded { lines.append("BVINPUT_KEYS \(id) SHIFT_ENTER_SPACE") }
        lines.append("BVAGENT END \(command)")
        _ = router.poll(binding: binding, serviceReady: true, lines: lines, now: now) { _ in false }
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { _ in false }
        XCTAssertEqual(router.state, .ready)
    }
}
