#if canImport(AppKit)
import CoreGraphics
import XCTest
@testable import BridgeVMControl

final class HvfPointerMoveMailboxTests: XCTestCase {
    func testBurstKeepsOnlyLatestPoint() {
        var mailbox = HvfPointerMoveMailbox()
        for value in 0 ..< 1_000 { mailbox.offer(CGPoint(x: value, y: value)) }

        XCTAssertEqual(mailbox.take(), CGPoint(x: 999, y: 999))
        XCTAssertNil(mailbox.take())
    }

    func testResetDropsPendingPoint() {
        var mailbox = HvfPointerMoveMailbox()
        mailbox.offer(CGPoint(x: 10, y: 20))
        mailbox.reset()
        XCTAssertNil(mailbox.take())
    }
}
#endif
