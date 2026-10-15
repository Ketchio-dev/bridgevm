import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfFramebufferLossInputTests: XCTestCase {
    private typealias Frame = HvfFramebufferAvailabilityFixture

    func testLossCancelsMarkedTextAndQueuedMoveOnceThenAcceptsFreshFrame() throws {
        try Frame.run { fixture, session, view, path in
            Frame.mark(view)
            view.mouseMoved(with: try Frame.mouse(.mouseMoved))
            let before = try fixture.commands(), target = session.textInputTarget
            try FileManager.default.removeItem(at: path)
            view.refreshFrame(at: 1)
            XCTAssertFalse(view.hasMarkedText())
            XCTAssertNotEqual(session.textInputTarget, target)
            let cancelledTarget = session.textInputTarget
            view.unmarkText()
            view.refreshFrame(at: 2)
            XCTAssertEqual(session.textInputTarget, cancelledTarget, "Repeated unavailable ticks must not recancel")
            XCTAssertEqual(try fixture.commands(), before, "Neither stale preedit nor queued pointer move may escape")
            try Frame.writeFrame(path)
            view.refreshFrame(at: 3)
            XCTAssertTrue(view.guestInputAvailable)
            XCTAssertEqual(view.accessibilityValue() as? String, "frame 1x1")
            view.insertText("fresh", replacementRange: NSRange(location: NSNotFound, length: 0))
            XCTAssertEqual(try fixture.commands().count, before.count + 1)
        }
    }

    func testMovesReceivedWhileUnavailableDoNotReplayIntoRecoveredFrame() throws {
        for type: NSEvent.EventType in [.mouseMoved, .leftMouseDragged, .rightMouseDragged] {
            try Frame.run { fixture, _, view, path in
                try FileManager.default.removeItem(at: path)
                view.refreshFrame(at: 1)
                let before = try fixture.commands()
                if type == .mouseMoved { view.mouseMoved(with: try Frame.mouse(type)) }
                else if type == .leftMouseDragged { view.mouseDragged(with: try Frame.mouse(type)) }
                else { view.rightMouseDragged(with: try Frame.mouse(type)) }
                try Frame.writeFrame(path)
                view.refreshFrame(at: 2)
                XCTAssertTrue(view.guestInputAvailable)
                XCTAssertEqual(try fixture.commands(), before,
                    "An event received without a display must not target a later frame")
            }
        }
    }

    func testFreshMoveAndBothDragSidesRemainEligibleAfterRecovery() throws {
        for type: NSEvent.EventType in [.mouseMoved, .leftMouseDragged, .rightMouseDragged] {
            try Frame.run { fixture, _, view, path in
                try FileManager.default.removeItem(at: path)
                view.refreshFrame(at: 1)
                try Frame.writeFrame(path)
                view.refreshFrame(at: 2)
                let before = try fixture.commands()
                if type == .mouseMoved { view.mouseMoved(with: try Frame.mouse(type)) }
                else if type == .leftMouseDragged { view.mouseDragged(with: try Frame.mouse(type)) }
                else { view.rightMouseDragged(with: try Frame.mouse(type)) }
                view.refreshFrame(at: 3)
                let after = try fixture.commands()
                XCTAssertEqual(after.count, before.count + 1)
                XCTAssertTrue(after.last?.hasPrefix("POINTERINPUT ") == true)
            }
        }
    }

    func testLossAttemptsHeldPointerReleaseAndDoesNotRepeatIt() throws {
        try Frame.run { fixture, session, view, path in
            view.mouseDown(with: try Frame.mouse(.leftMouseDown))
            let press = try XCTUnwrap(try fixture.commands().last)
            XCTAssertTrue(press.hasPrefix("POINTERINPUT "))
            try FileManager.default.removeItem(at: path)
            view.refreshFrame(at: 1)
            let after = try fixture.commands()
            XCTAssertEqual(after.count, 3, "Capability, original press, then one correlated cleanup")
            let cleanup = try XCTUnwrap(after.last)
            XCTAssertNotEqual(cleanup, press)
            let payload = try XCTUnwrap(Data(base64Encoded: String(cleanup.split(separator: " ")[2])))
            XCTAssertTrue(String(decoding: payload, as: UTF8.self).hasPrefix("releaseall:"))
            XCTAssertFalse(view.guestInputAvailable)
            let target = session.textInputTarget
            view.refreshFrame(at: 2)
            XCTAssertEqual(try fixture.commands(), after)
            XCTAssertEqual(session.textInputTarget, target)
        }
    }
}
