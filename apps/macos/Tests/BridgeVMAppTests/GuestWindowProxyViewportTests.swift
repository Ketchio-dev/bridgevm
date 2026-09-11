import XCTest

@testable import BridgeVMApp

final class GuestWindowProxyViewportTests: XCTestCase {
  private func plan() throws -> GuestWindowProxyPlan {
    try GuestWindowProxyPlanner.plan(
      vmName: "VM",
      window: GuestToolsWindowAction(
        id: "1", title: "Editor",
        bounds: GuestToolsWindowBounds(x: 100, y: 50, width: 800, height: 600)
      )
    )
  }

  func testResizedViewportsMapTheSameImagePoint() throws {
    let plan = try plan()
    for scale in [0.5, 1.0, 2.0] {
      let point = plan.viewportPoint(
        .init(x: 200 * scale, y: 150 * scale), width: 800 * scale, height: 600 * scale
      )
      XCTAssertEqual(point, .init(x: 300, y: 200))
    }
  }

  func testCenteredLetterboxingOnBothAxes() throws {
    let plan = try plan()
    XCTAssertEqual(plan.viewportPoint(.init(x: 400, y: 150), width: 1200, height: 600),
      .init(x: 300, y: 200))
    XCTAssertEqual(plan.viewportPoint(.init(x: 200, y: 450), width: 800, height: 1200),
      .init(x: 300, y: 200))
  }

  func testDraggingOutsideImageClampsToGuestEdges() throws {
    let plan = try plan()
    XCTAssertEqual(plan.viewportPoint(.init(x: -100, y: -100), width: 1200, height: 600),
      .init(x: 100, y: 50))
    XCTAssertEqual(plan.viewportPoint(.init(x: 2000, y: 2000), width: 1200, height: 600),
      .init(x: 899, y: 649))
  }

  func testInvalidViewportAndPointerAreRejected() throws {
    let plan = try plan()
    for value in [0.0, -1.0, Double.infinity, Double.nan] {
      XCTAssertNil(plan.viewportPoint(.init(x: 0, y: 0), width: value, height: 600))
      XCTAssertNil(plan.viewportPoint(.init(x: 0, y: 0), width: 800, height: value))
    }
    XCTAssertNil(plan.viewportPoint(.init(x: .nan, y: 0), width: 800, height: 600))
    XCTAssertNil(plan.viewportPoint(.init(x: 0, y: .infinity), width: 800, height: 600))
  }
}
