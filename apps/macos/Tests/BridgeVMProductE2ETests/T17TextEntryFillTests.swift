import XCTest
@testable import BridgeVMProductE2E

final class T17TextEntryFillTests: XCTestCase {
    func testFocusPrecedesTheValueAndNothingConfirmsIt() throws {
        var events: [String] = [], stored = ""
        try T17TextEntry.fill("CLIPGET", focus: { events.append("focus"); return true },
                              set: { stored = $0; events.append("set"); return true }, read: { events.append("read"); return stored })
        XCTAssertEqual(events, ["focus", "set", "read"])
    }

    func testRefusedFocusSetsNothing() {
        var setCalled = false
        XCTAssertThrowsError(try T17TextEntry.fill("CLIPGET", focus: { false }, set: { _ in setCalled = true; return true }, read: { "CLIPGET" })) {
            XCTAssertEqual(($0 as? T17Blocker)?.detail, "identified UI element did not take focus")
        }
        XCTAssertFalse(setCalled)
    }

    func testChangedReadBackFails() {
        XCTAssertThrowsError(try T17TextEntry.fill("CLIPGET", focus: { true }, set: { _ in true }, read: { "" })) {
            XCTAssertEqual(($0 as? T17Blocker)?.detail, "identified UI element did not retain exact text")
        }
    }
}
