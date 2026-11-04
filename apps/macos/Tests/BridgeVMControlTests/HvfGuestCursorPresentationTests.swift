#if canImport(AppKit)
import CoreGraphics
import XCTest
@testable import BridgeVMControl

final class HvfGuestCursorPresentationTests: XCTestCase {
    /// A 200x100 guest in a 100x100 view: the image is 100x50 at y 25 (black bars above and below).
    private let imageSize = CGSize(width: 200, height: 100)
    private lazy var fit = HvfDisplayFit(viewSize: CGSize(width: 100, height: 100), imageSize: imageSize)
    private let shown = HvfGuestCursorState(serial: 1, visible: true, shapeThisBoot: true, x: 100, y: 50,
                                            hotX: 2, hotY: 4, width: 64, height: 64)

    private func decide(_ cursor: HvfGuestCursorState?, key: Bool = true,
                        pointer: CGPoint? = CGPoint(x: 50, y: 50)) -> HvfGuestCursorPresentation {
        HvfGuestCursorPresentation.decide(cursor: cursor, keyWindow: key, pointer: pointer, fit: fit,
                                          imageSize: imageSize)
    }

    func testThePointerOnTheImageOfTheKeyWindowTakesTheGuestImage() {
        XCTAssertEqual(decide(shown), HvfGuestCursorPresentation(macCursor: true, sprite: nil))
        // The image's edges count, exactly where pointer input still goes to the guest.
        for edge in [CGPoint(x: 0, y: 25), CGPoint(x: 100, y: 75)] {
            XCTAssertTrue(decide(shown, pointer: edge).macCursor)
            XCTAssertNotNil(HvfDisplayCoordinates.absolutePointer(
                location: edge, viewSize: CGSize(width: 100, height: 100), imageSize: imageSize))
        }
    }

    func testOffTheImageOrInAnInactiveWindowTheSpriteIsDrawnAtTheGuestPosition() {
        // The sprite's top left (100, 50) at half scale, from the image's origin (0, 25).
        let sprite = HvfGuestCursorPresentation(macCursor: false, sprite: CGRect(x: 50, y: 50, width: 32, height: 32))
        XCTAssertEqual(decide(shown, pointer: CGPoint(x: 50, y: 10)), sprite, "black bar")
        XCTAssertEqual(decide(shown, pointer: nil), sprite, "outside the view")
        XCTAssertEqual(decide(shown, key: false), sprite, "the window is not key")
    }

    func testNothingShowsWithoutAVisibleShapeOrOutsideTheGuestFrame() {
        var hidden = shown
        hidden.visible = false
        var earlierBoot = shown
        earlierBoot.shapeThisBoot = false
        var outside = shown
        outside.x = 199  // the pointer, at the hot spot, is at 201: off the 200-pixel guest frame
        for cursor in [nil, hidden, earlierBoot] {
            XCTAssertEqual(decide(cursor), .none)
            XCTAssertEqual(decide(cursor, pointer: nil), .none)
        }
        XCTAssertEqual(decide(outside, pointer: nil), .none)
        // WDDM's position is the image's top left, negative for a centred hot spot at the edge.
        var edge = shown
        (edge.x, edge.y, edge.hotX, edge.hotY) = (-8, -8, 8, 8)
        XCTAssertEqual(decide(edge, pointer: nil).sprite, CGRect(x: -4, y: 21, width: 32, height: 32))
        XCTAssertEqual(HvfGuestCursorPresentation.decide(cursor: shown, keyWindow: true, pointer: CGPoint(x: 50, y: 50),
                                                         fit: nil, imageSize: .zero), .none, "no frame yet")
    }

    func testOnlyARunningVMsFileCounts() {
        XCTAssertTrue(HvfConnectionState.booting.guestCursorIsLive)
        XCTAssertTrue(HvfConnectionState.connected(host: "guest").guestCursorIsLive)
        for state in [HvfConnectionState.stopped, .stopping, .timedOut] {
            XCTAssertFalse(state.guestCursorIsLive)
        }
    }
}
#endif
