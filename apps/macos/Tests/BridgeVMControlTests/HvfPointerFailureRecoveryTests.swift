import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfPointerFailureRecoveryTests: XCTestCase {
    func testConfirmedPressGetsOneCleanupWithoutReopeningUserInput() {
        var fixture = HvfPointerFailureFixture()
        fixture.failAfterConfirmedPress()
        XCTAssertEqual(fixture.router.count, 1)
        fixture.assertFailedClosed()
        _ = fixture.poll()
        let cleanup = fixture.commands.last!
        XCTAssertEqual(fixture.payloads(), ["press:1x2", "fail", "releaseall:1x2"])
        _ = fixture.router.cancelTarget(now: fixture.now)
        XCTAssertEqual(fixture.poll(), .waiting)
        _ = fixture.poll(lines: fixture.receipt(cleanup))
        XCTAssertEqual(fixture.router.count, 0)
        fixture.assertFailedClosed()
        _ = fixture.router.cancelTarget(now: fixture.now)
        XCTAssertNil(fixture.poll())
        XCTAssertEqual(fixture.commands.count, 3)
    }

    func testExpiredUnacknowledgedPressPrecedesCleanupAndItsLateReceiptCannotCompleteCleanup() {
        var fixture = HvfPointerFailureFixture()
        let press = fixture.send(.pointer("rightpress:3x4"))
        XCTAssertEqual(fixture.router.route(.text("discard"), binding: fixture.binding, now: fixture.now), .queued)
        fixture.now += 31
        XCTAssertEqual(fixture.poll(), .cancelled(.expired, discarded: 2))
        XCTAssertEqual(fixture.router.count, 1)
        _ = fixture.poll()
        let cleanup = fixture.commands.last!
        XCTAssertNotEqual(press.split(separator: " ")[1], cleanup.split(separator: " ")[1])
        XCTAssertEqual(fixture.payloads(), ["rightpress:3x4", "releaseall:3x4"])
        XCTAssertEqual(fixture.poll(lines: fixture.receipt(press)), .waiting)
        XCTAssertEqual(fixture.router.count, 1)
        _ = fixture.poll(lines: fixture.receipt(cleanup))
        XCTAssertEqual(fixture.router.count, 0)
        fixture.assertFailedClosed()
        XCTAssertEqual(fixture.commands.count, 2, "The expired press must never be retransmitted")
    }

    func testCleanupWriteRejectionInvalidReceiptAndExpiryNeverRetry() {
        for failure in ["write", "receipt", "expiry"] {
            var fixture = HvfPointerFailureFixture()
            fixture.failAfterConfirmedPress()
            _ = fixture.poll(send: failure != "write")
            if failure == "receipt" { _ = fixture.poll(lines: fixture.receipt(fixture.commands.last!, inserted: 0)) }
            if failure == "expiry" { fixture.now += 31; _ = fixture.poll() }
            XCTAssertEqual(fixture.router.count, 0, failure)
            fixture.assertFailedClosed()
            _ = fixture.router.cancelTarget(now: fixture.now)
            XCTAssertNil(fixture.poll())
            XCTAssertEqual(fixture.commands.count, 3, failure)
        }
    }

    func testNoSentOrAlreadyReleasedPressDoesNotScheduleCleanup() {
        for released in [false, true] {
            var fixture = HvfPointerFailureFixture()
            if released {
                let press = fixture.send(.pointer("press:1x2"))
                _ = fixture.poll(lines: fixture.receipt(press))
                let release = fixture.send(.pointer("releaseall:1x2"))
                _ = fixture.poll(lines: fixture.receipt(release))
            }
            let text = fixture.send(.text("fail"))
            _ = fixture.poll(lines: fixture.receipt(text, inserted: nil))
            XCTAssertEqual(fixture.router.count, 0)
            let before = fixture.commands
            _ = fixture.router.cancelTarget(now: fixture.now)
            XCTAssertNil(fixture.poll())
            XCTAssertEqual(fixture.commands, before)
            fixture.assertFailedClosed()
        }
    }
}
