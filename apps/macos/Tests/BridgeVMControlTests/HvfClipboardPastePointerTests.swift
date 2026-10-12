#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

/// A press can move guest focus without changing the host first responder, so the
/// delayed legacy `KEY ctrl+v` must not follow it into whatever the press focused.
extension HvfClipboardPasteTargetTests {
    private static let size = CGSize(width: 100, height: 100), at = CGPoint(x: 5, y: 5)

    func testPointerPressDuringLegacyPasteCancelsTheDelayedPaste() throws {
        let presses: [(String, (HvfEngineSession) -> Void)] = [
            ("click", { $0.sendPointerClick(location: Self.at, viewSize: Self.size, imageSize: Self.size) }),
            ("press", { $0.sendPointerPress(location: Self.at, viewSize: Self.size, imageSize: Self.size) }),
            ("right-click", { $0.sendPointerRightClick(location: Self.at, viewSize: Self.size, imageSize: Self.size) }),
            ("right-press", { $0.sendPointerRightPress(location: Self.at, viewSize: Self.size, imageSize: Self.size) }),
        ]
        for (action, press) in presses {
            try withSession { session, root in
                XCTAssertEqual(session.sendText("한글"), .acceptedForProcessing)
                let command = try lastCommand(session)
                press(session)
                try acknowledge(command, root: root)
                session.poll()
                let input = try String(contentsOf: root.appendingPathComponent("input.ctl"), encoding: .utf8)
                XCTAssertEqual(input, "POINTER \(action):1638x1638\n", action)
                XCTAssertTrue(session.events.contains(.unknown("clipboard paste canceled: pointer pressed")), action)
                XCTAssertEqual(session.sendText("다시"), .acceptedForProcessing, action)
            }
        }
    }

    func testPointerMoveScrollAndReleaseKeepTheLegacyPastePending() throws {
        try withSession { session, root in
            XCTAssertEqual(session.sendText("한글"), .acceptedForProcessing)
            let command = try lastCommand(session)
            session.sendPointerMove(location: Self.at, viewSize: Self.size, imageSize: Self.size)
            session.sendPointerScroll(1, location: Self.at, viewSize: Self.size, imageSize: Self.size)
            session.sendPointerRelease(location: Self.at, viewSize: Self.size, imageSize: Self.size)
            try acknowledge(command, root: root)
            session.poll()
            XCTAssertEqual(try String(contentsOf: root.appendingPathComponent("input.ctl"), encoding: .utf8),
                           "POINTER move:1638x1638\nPOINTER scroll:1@1638x1638\nPOINTER release:1638x1638\nKEY ctrl+v\n")
            XCTAssertFalse(session.events.contains { $0.displayText.contains("clipboard paste canceled") })
        }
    }
}
#endif
