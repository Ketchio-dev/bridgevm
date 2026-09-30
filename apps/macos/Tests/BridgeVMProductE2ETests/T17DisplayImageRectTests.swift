import XCTest
@testable import BridgeVMProductE2E

final class T17DisplayImageRectTests: XCTestCase {
    func testGuestSizeComesOnlyFromAnExactFrameValue() {
        XCTAssertEqual(T17DisplayImageRect.guestSize("frame 800x600"), CGSize(width: 800, height: 600))
        for value in [nil, "no-frame", "frame 800x", "frame x600", "frame 0x600", "frame 800x600x2", "frame -8x600", "frames 800x600"] {
            XCTAssertNil(T17DisplayImageRect.guestSize(value), String(describing: value))
        }
    }

    func testSpotsMapInsideTheLetterboxedGuestImage() {
        // A 4:3 guest in a 1280x800 surface is 1066.7 wide with a 106.7-point bar on each side.
        let surface = CGRect(x: 100, y: 50, width: 1280, height: 800), guest = CGSize(width: 800, height: 600)
        let centre = T17DisplayImageRect.point(CGPoint(x: 0.5, y: 0.5), guest: guest, in: surface)
        XCTAssertEqual(centre, CGPoint(x: 740, y: 450))
        let beside = T17DisplayImageRect.point(CGPoint(x: 0.04, y: 0.5), guest: guest, in: surface)!
        XCTAssertEqual(beside.x, 100 + 1280.0 / 2 - 1066.6667 / 2 + 0.04 * 1066.6667, accuracy: 0.01)
        XCTAssertEqual(beside.y, 450)
        XCTAssertGreaterThan(beside.x, 100 + 106.6667, "the click lands on the image, not the bar")
    }

    func testSpotsOutsideTheImageOrAnEmptySurfaceAreRefused() {
        let guest = CGSize(width: 800, height: 600)
        XCTAssertNil(T17DisplayImageRect.point(CGPoint(x: 1.1, y: 0.5), guest: guest, in: CGRect(x: 0, y: 0, width: 800, height: 600)))
        XCTAssertNil(T17DisplayImageRect.point(CGPoint(x: 0.5, y: -0.1), guest: guest, in: CGRect(x: 0, y: 0, width: 800, height: 600)))
        XCTAssertNil(T17DisplayImageRect.point(CGPoint(x: 0.5, y: 0.5), guest: guest, in: .zero))
    }
}
