import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfFramebufferAvailabilityTests: XCTestCase {
    private typealias Frame = HvfFramebufferAvailabilityFixture

    func testRemovedFramebufferStopsViewInputBeforeTheNextAppKitCallback() throws {
        try checkInput(removeFrame: true)
    }

    func testUnchangedFramebufferKeepsViewInputAvailable() throws {
        try checkInput(removeFrame: false)
    }

    func testInvalidHeaderOrTruncatedBackingInvalidatesInputAndCachedImage() throws {
        for invalid in ["magic", "dimensions", "truncated"] {
            try Frame.run { fixture, _, view, path in
                let before = try fixture.commands()
                if invalid == "magic" { try Frame.update(UInt32(0), offset: 0, path: path) }
                if invalid == "dimensions" {
                    try Frame.update(UInt32(0), offset: 8, path: path)
                    try Frame.update(UInt64(4), offset: 24, path: path)
                }
                if invalid == "truncated" {
                    let handle = try FileHandle(forWritingTo: path)
                    try handle.truncate(atOffset: 64); try handle.close()
                }
                view.refreshFrame(at: 1)
                XCTAssertFalse(view.guestInputAvailable, invalid)
                XCTAssertNil(view.layer?.contents, invalid)
                XCTAssertEqual(view.accessibilityValue() as? String, "no-frame", invalid)
                XCTAssertEqual(try fixture.commands(), before, invalid)
            }
        }
    }

    private func checkInput(removeFrame: Bool) throws {
        try Frame.run { fixture, _, view, path in
            let before = try fixture.commands()
            if removeFrame { try FileManager.default.removeItem(at: path) }
            view.refreshFrame(at: 1)
            XCTAssertEqual(view.guestInputAvailable, !removeFrame)
            view.insertText("late", replacementRange: NSRange(location: NSNotFound, length: 0))
            view.mouseDown(with: try Frame.mouse(.leftMouseDown))
            let after = try fixture.commands()
            if removeFrame {
                XCTAssertEqual(after, before, "Stale display callbacks must not emit new guest input")
                XCTAssertNil(view.layer?.contents)
            } else {
                XCTAssertEqual(after.count, before.count + 1)
                XCTAssertTrue(after.last?.hasPrefix("TEXTINPUT ") == true)
            }
            XCTAssertNil(try fixture.bytes(fixture.input))
        }
    }
}
