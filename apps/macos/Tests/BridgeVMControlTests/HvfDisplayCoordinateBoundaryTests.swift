#if canImport(AppKit)
import CoreGraphics
import XCTest
@testable import BridgeVMControl

final class HvfDisplayCoordinateBoundaryTests: XCTestCase {
    func testNonfiniteCoordinatesAndDimensionsAreRejected() {
        for invalid in [CGFloat.nan, .infinity, -.infinity] {
            for component in 0..<6 {
                var point = CGPoint(x: 50, y: 50)
                var view = CGSize(width: 100, height: 100)
                var image = view
                switch component {
                case 0: point.x = invalid
                case 1: point.y = invalid
                case 2: view.width = invalid
                case 3: view.height = invalid
                case 4: image.width = invalid
                default: image.height = invalid
                }
                XCTAssertNil(HvfDisplayCoordinates.absolutePointer(location: point, viewSize: view, imageSize: image))
            }
        }
    }

    func testUnderflowedDisplayDimensionsAreRejected() {
        let tiny = CGFloat.leastNonzeroMagnitude
        XCTAssertNil(HvfDisplayCoordinates.absolutePointer(
            location: .zero, viewSize: CGSize(width: tiny, height: tiny),
            imageSize: CGSize(width: 1_000, height: 1_000)))
        XCTAssertNil(HvfDisplayCoordinates.absolutePointer(
            location: CGPoint(x: 500, y: 500), viewSize: CGSize(width: 1_000, height: 1_000),
            imageSize: CGSize(width: .greatestFiniteMagnitude, height: tiny)))
    }

    func testRepresentableExtremeSizesStillMapTheCenter() {
        for size in [CGFloat.leastNormalMagnitude, .greatestFiniteMagnitude, 1_000] {
            let result = HvfDisplayCoordinates.absolutePointer(
                location: CGPoint(x: size / 2, y: size / 2),
                viewSize: CGSize(width: size, height: size), imageSize: CGSize(width: size, height: size))
            XCTAssertEqual(result?.x, 16_384)
            XCTAssertEqual(result?.y, 16_384)
        }
    }
}
#endif
