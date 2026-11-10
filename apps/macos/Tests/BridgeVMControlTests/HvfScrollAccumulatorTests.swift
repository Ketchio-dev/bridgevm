import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfScrollAccumulatorTests: XCTestCase {
    private typealias Delta = HvfScrollAccumulator.Delta

    private func total(_ deltas: [Delta], pixelsPerPoint: Double = 2, notches: Bool = false,
                       into accumulator: inout HvfScrollAccumulator) -> (vertical: Int, horizontal: Int, events: Int) {
        var sum = (vertical: 0, horizontal: 0, events: 0)
        for delta in deltas {
            guard let units = accumulator.add(delta, guestPixelsPerPoint: pixelsPerPoint, wholeNotches: notches) else { continue }
            sum.vertical += units.vertical; sum.horizontal += units.horizontal; sum.events += 1
        }
        return sum
    }

    /// The defect: every precise event was rounded up to at least one whole notch (here 100 notches).
    func testATrackpadGestureIsProportionalToFingerTravel() {
        var accumulator = HvfScrollAccumulator()
        let gesture = [Delta(x: 0, y: 0, precise: true, beginsGesture: true)]
            + Array(repeating: Delta(x: 0, y: -2, precise: true), count: 50)
        let sum = total(gesture, into: &accumulator)
        // 100 points x 2 guest pixels per point x 1.6 = 320 units (2.7 notches), no fraction rounded up.
        XCTAssertLessThan(abs(sum.vertical + 320), 2, "\(sum.vertical)")
        XCTAssertEqual(sum.horizontal, 0)
        var legacy = HvfScrollAccumulator()
        XCTAssertEqual(total(gesture, notches: true, into: &legacy).vertical, -240, "two whole notches on the HID wheel")
    }

    func testAMomentumTailAddsAlmostNothing() {
        var accumulator = HvfScrollAccumulator()
        let tail = Array(repeating: Delta(x: 0, y: 0.3, precise: true), count: 40)
        let sum = total(tail, into: &accumulator)
        XCTAssertLessThan(sum.vertical, 40, "40 x 0.3 points x 2 x 1.6 = 38.4 units")
        XCTAssertGreaterThanOrEqual(sum.vertical, 38)
        var legacy = HvfScrollAccumulator()
        XCTAssertEqual(total(tail, notches: true, into: &legacy).events, 0, "not one notch")
    }

    func testAMouseWheelLineIsOneNotchAndFractionsAccumulate() {
        var accumulator = HvfScrollAccumulator()
        for notches in [false, true] {
            XCTAssertEqual(accumulator.add(Delta(x: 0, y: 1, precise: false), guestPixelsPerPoint: 2, wholeNotches: notches),
                           .init(vertical: 120, horizontal: 0))
            XCTAssertEqual(accumulator.add(Delta(x: 0, y: -3, precise: false), guestPixelsPerPoint: 2, wholeNotches: notches),
                           .init(vertical: -360, horizontal: 0))
        }
        XCTAssertNil(accumulator.add(Delta(x: 0, y: -0.5, precise: false), guestPixelsPerPoint: 2, wholeNotches: true))
        XCTAssertEqual(accumulator.add(Delta(x: 0, y: -0.5, precise: false), guestPixelsPerPoint: 2, wholeNotches: true),
                       .init(vertical: -120, horizontal: 0), "two half lines are one notch, not two")
    }

    func testADirectionFlipOrANewGestureDropsTheRemainder() {
        var flip = HvfScrollAccumulator()
        XCTAssertNil(flip.add(Delta(x: 0, y: 100.0 / 120, precise: false), guestPixelsPerPoint: 1, wholeNotches: true))
        XCTAssertNil(flip.add(Delta(x: 0, y: -0.5, precise: false), guestPixelsPerPoint: 1, wholeNotches: true))
        XCTAssertEqual(flip.add(Delta(x: 0, y: -0.5, precise: false), guestPixelsPerPoint: 1, wholeNotches: true),
                       .init(vertical: -120, horizontal: 0), "the 100 units up do not cancel the way down")
        var gesture = HvfScrollAccumulator()
        XCTAssertNil(gesture.add(Delta(x: 0, y: 30, precise: true), guestPixelsPerPoint: 2, wholeNotches: true)) // 96
        XCTAssertNil(gesture.add(Delta(x: 0, y: 10, precise: true, beginsGesture: true), guestPixelsPerPoint: 2,
                                 wholeNotches: true), "a new gesture starts from 32, not 128")
    }

    /// Windows: WHEEL > 0 scrolls up, HWHEEL > 0 scrolls right. AppKit deltas already carry the
    /// natural-scrolling choice: deltaY > 0 asks to scroll up, deltaX > 0 to scroll left.
    func testSignsMatchWindowsWheelAndHorizontalWheel() {
        var accumulator = HvfScrollAccumulator()
        XCTAssertEqual(accumulator.add(Delta(x: 10, y: 10, precise: true), guestPixelsPerPoint: 2),
                       .init(vertical: 32, horizontal: -32))
        XCTAssertEqual(accumulator.add(Delta(x: -10, y: -10, precise: true), guestPixelsPerPoint: 2),
                       .init(vertical: -32, horizontal: 32))
        XCTAssertEqual(accumulator.add(Delta(x: -1, y: 0, precise: false), guestPixelsPerPoint: 2),
                       .init(vertical: 0, horizontal: 120))
        XCTAssertNil(accumulator.add(Delta(x: -1, y: 0, precise: false), guestPixelsPerPoint: 2, wholeNotches: true),
                     "the HID wheel has no horizontal axis")
    }

    func testEachEventIsBoundedAndNothingInvalidIsEmitted() {
        var accumulator = HvfScrollAccumulator()
        XCTAssertEqual(accumulator.add(Delta(x: -50, y: 50, precise: false), guestPixelsPerPoint: 2),
                       .init(vertical: 1_200, horizontal: 1_200))
        XCTAssertNil(accumulator.add(Delta(x: 0, y: 0.001, precise: false), guestPixelsPerPoint: 2), "the excess is not carried")
        XCTAssertNil(accumulator.add(Delta(x: 0, y: 0, precise: true), guestPixelsPerPoint: 2))
        XCTAssertNil(accumulator.add(Delta(x: .nan, y: .infinity, precise: true), guestPixelsPerPoint: 2))
        XCTAssertNil(accumulator.add(Delta(x: 0, y: 1, precise: true), guestPixelsPerPoint: .nan))
        XCTAssertEqual(accumulator.add(Delta(x: 0, y: 1, precise: false), guestPixelsPerPoint: 2),
                       .init(vertical: 120, horizontal: 0), "an invalid delta leaves no poisoned remainder")
    }
}
