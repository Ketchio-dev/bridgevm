import XCTest

@testable import BridgeVMApp

/// A guest window used to open centred on the host screen regardless of where it
/// sat inside the guest, so several guest windows landed on top of each other.
final class GuestWindowProxyOriginTests: XCTestCase {
  func testHostOriginKeepsTheGuestPositionWithTheYAxisFlipped() throws {
    let window = GuestToolsWindowAction(
      id: "window-1",
      title: "Editor",
      bounds: GuestToolsWindowBounds(x: 200, y: 150, width: 800, height: 600)
    )
    let plan = try GuestWindowProxyPlanner.plan(vmName: "Dev VM", window: window)
    let screen = GuestWindowProxyPlan.HostFrame(x: 0, y: 0, width: 1920, height: 1080)

    let origin = plan.hostOrigin(inVisibleFrame: screen)

    // Unscaled, so x carries over directly and y is measured from the bottom:
    // 1080 - (150 + 600) = 330.
    XCTAssertEqual(origin.x, 200, accuracy: 0.001)
    XCTAssertEqual(origin.y, 330, accuracy: 0.001)
  }

  func testHostOriginScalesWithADownscaledWindow() throws {
    let window = GuestToolsWindowAction(
      id: "window-1",
      title: "Huge",
      bounds: GuestToolsWindowBounds(x: 400, y: 200, width: 2880, height: 1800)
    )
    let plan = try GuestWindowProxyPlanner.plan(vmName: "Dev VM", window: window)
    let screen = GuestWindowProxyPlan.HostFrame(x: 0, y: 0, width: 1920, height: 1080)

    let origin = plan.hostOrigin(inVisibleFrame: screen)

    // The planner fits 2880x1800 into 1440x900, so scale is 0.5 and the guest
    // origin has to scale with it rather than being used raw.
    XCTAssertEqual(plan.scale, 0.5, accuracy: 0.001)
    XCTAssertEqual(origin.x, 200, accuracy: 0.001)
    XCTAssertEqual(origin.y, 1080 - (100 + 900), accuracy: 0.001)
  }

  func testHostOriginClampsAWindowPositionedOffTheGuestDesktop() throws {
    let window = GuestToolsWindowAction(
      id: "window-1",
      title: "Far away",
      bounds: GuestToolsWindowBounds(x: 5000, y: 4000, width: 800, height: 600)
    )
    let plan = try GuestWindowProxyPlanner.plan(vmName: "Dev VM", window: window)
    let screen = GuestWindowProxyPlan.HostFrame(x: 0, y: 0, width: 1920, height: 1080)

    let origin = plan.hostOrigin(inVisibleFrame: screen)

    XCTAssertEqual(origin.x, 1920 - 800, accuracy: 0.001)
    XCTAssertEqual(origin.y, 0, accuracy: 0.001)
  }

  func testHostOriginIsRelativeToTheVisibleFrameNotTheScreen() throws {
    let window = GuestToolsWindowAction(
      id: "window-1",
      title: "Editor",
      bounds: GuestToolsWindowBounds(x: 0, y: 0, width: 800, height: 600)
    )
    let plan = try GuestWindowProxyPlanner.plan(vmName: "Dev VM", window: window)
    // A visible frame that excludes a menu bar and a Dock.
    let visible = GuestWindowProxyPlan.HostFrame(x: 0, y: 80, width: 1920, height: 950)

    let origin = plan.hostOrigin(inVisibleFrame: visible)

    XCTAssertEqual(origin.x, 0, accuracy: 0.001)
    XCTAssertEqual(origin.y, 80 + 950 - 600, accuracy: 0.001)
  }

  /// Moving a proxy window sends new bounds to the guest, which reports them
  /// back, which rebuilds the plan. The rebuilt window has to land where the
  /// user just put it rather than jumping somewhere else.
  func testMovingAProxyWindowRoundTripsBackToTheSamePlace() throws {
    let window = GuestToolsWindowAction(
      id: "window-1",
      title: "Editor",
      bounds: GuestToolsWindowBounds(x: 200, y: 150, width: 800, height: 600)
    )
    let plan = try GuestWindowProxyPlanner.plan(vmName: "Dev VM", window: window)
    let screen = GuestWindowProxyPlan.HostFrame(x: 0, y: 0, width: 1920, height: 1080)
    let start = plan.hostOrigin(inVisibleFrame: screen)

    // The user drags it 100 points right and 50 points up.
    let moved = GuestWindowProxyPlan.HostFrame(
      x: start.x + 100, y: start.y + 50, width: 800, height: 600
    )
    let baseline = GuestWindowProxyPlan.HostFrame(
      x: start.x, y: start.y, width: 800, height: 600
    )
    let reported = plan.guestBounds(forHostContentFrame: moved, relativeTo: baseline)

    let rebuilt = try GuestWindowProxyPlanner.plan(
      vmName: "Dev VM",
      window: GuestToolsWindowAction(id: "window-1", title: "Editor", bounds: reported)
    )
    let settled = rebuilt.hostOrigin(inVisibleFrame: screen)

    XCTAssertEqual(settled.x, moved.x, accuracy: 0.001)
    XCTAssertEqual(settled.y, moved.y, accuracy: 0.001)
  }
}
