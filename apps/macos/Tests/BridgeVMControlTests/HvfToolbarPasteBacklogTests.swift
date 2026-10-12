#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfToolbarPasteBacklogTests: XCTestCase, HvfLegacyPasteFixture {
    func testToolbarKeysWaitBehindPasteAndRefuseVisiblyAtCapacityWithoutChangingDraft() throws {
        try withSession { session, root in
            XCTAssertNotEqual(session.sendText("한"), .refused)
            let first = try lastCommand(session)
            var draft = "unsent draft"
            for key in ["tab", "enter", "space", "esc", "backspace", "delete", "left", "up", "down", "right", "home", "end", "ctrl+alt+delete"] {
                XCTAssertTrue(session.sendToolbarKey(key), key)
            }
            XCTAssertEqual(HvfKeyboardDraft.submit(&draft, to: session), .refused)
            XCTAssertEqual(draft, "unsent draft")
            XCTAssertEqual(session.legacyPaste.backlog.count, 13)
            XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("input.ctl").path))
            for _ in 13..<HvfLegacyPasteState.maximumBacklog {
                XCTAssertTrue(session.sendToolbarKey("tab"))
            }
            XCTAssertFalse(session.sendToolbarKey("space"))
            XCTAssertEqual(session.events.last, .unknown("display input refused: 256 entries already wait for a clipboard paste"))
            try acknowledge(first, root: root); session.poll()
            let input = try String(contentsOf: root.appendingPathComponent("input.ctl"), encoding: .utf8)
            XCTAssertEqual(input, "KEY ctrl+v\n" + ["tab", "enter", "space", "esc", "backspace", "delete", "left", "up", "down", "right", "home", "end", "ctrl+alt+delete"].map { "KEY \($0)\n" }.joined() + String(repeating: "KEY tab\n", count: HvfLegacyPasteState.maximumBacklog - 13))
            XCTAssertEqual(draft, "unsent draft")
        }
    }
}
#endif
