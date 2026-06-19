import XCTest

@testable import BridgeVMApp

final class GuestWindowProxyPlannerTests: XCTestCase {
  func testPlanUsesGuestBoundsAtNativeScaleWhenTheyFit() throws {
    let window = GuestToolsWindowAction(
      id: "0x01200007",
      title: "Terminal",
      source: "wmctrl",
      focused: true,
      desktop: 0,
      pid: 4242,
      bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600)
    )

    let plan = try GuestWindowProxyPlanner.plan(vmName: "Dev VM", window: window)

    XCTAssertEqual(plan.vmName, "Dev VM")
    XCTAssertEqual(plan.windowID, "0x01200007")
    XCTAssertEqual(plan.title, "Terminal")
    XCTAssertEqual(plan.guestBounds, GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600))
    XCTAssertEqual(plan.hostSize, GuestWindowProxyPlan.HostSize(width: 800, height: 600))
    XCTAssertEqual(plan.scale, 1.0)
    XCTAssertEqual(plan.inputScaleX, 1.0)
    XCTAssertEqual(plan.inputScaleY, 1.0)
    XCTAssertEqual(plan.pid, 4242)
    XCTAssertEqual(plan.desktop, 0)
    XCTAssertNil(plan.cropFrameSummaryPath)
  }

  func testPlanCapsLargeGuestWindowToHostMaximumAndPreservesInputScale() throws {
    let window = GuestToolsWindowAction(
      id: "0x02000010",
      title: "Large Window",
      source: "wmctrl",
      bounds: GuestToolsWindowBounds(x: 0, y: 0, width: 3000, height: 2000)
    )

    let plan = try GuestWindowProxyPlanner.plan(
      vmName: "Dev VM",
      window: window,
      maximumHostSize: GuestWindowProxyPlan.HostSize(width: 1500, height: 1000)
    )

    XCTAssertEqual(plan.hostSize, GuestWindowProxyPlan.HostSize(width: 1500, height: 1000))
    XCTAssertEqual(plan.scale, 0.5)
    XCTAssertEqual(plan.inputScaleX, 2.0)
    XCTAssertEqual(plan.inputScaleY, 2.0)
  }

  func testPlanBuildsDisplaydWindowRegionArguments() throws {
    let window = GuestToolsWindowAction(
      id: "0x01200007",
      title: "Real Terminal",
      source: "wmctrl",
      bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600)
    )
    let plan = try GuestWindowProxyPlanner.plan(
      vmName: "Dev VM",
      window: window,
      maximumHostSize: GuestWindowProxyPlan.HostSize(width: 400, height: 300)
    )

    XCTAssertEqual(plan.minimumFramebufferSize, GuestWindowProxyPlan.FramebufferSize(width: 830, height: 640))
    XCTAssertEqual(
      plan.displaydWindowRegionArguments(
        framebufferSize: GuestWindowProxyPlan.FramebufferSize(width: 1440, height: 900),
        backingScale: 2
      ),
      [
        "--framebuffer-width",
        "1440",
        "--framebuffer-height",
        "900",
        "--scale",
        "2",
        "--window-id",
        "0x01200007",
        "--window-title",
        "Real Terminal",
        "--window-x",
        "30",
        "--window-y",
        "40",
        "--window-width",
        "800",
        "--window-height",
        "600",
        "--window-host-width",
        "400",
        "--window-host-height",
        "300",
      ]
    )
  }

  func testPlanBuildsDisplaydWindowCropArguments() throws {
    let window = GuestToolsWindowAction(
      id: "0x01200007",
      title: "Real Terminal",
      source: "wmctrl",
      bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600)
    )
    let plan = try GuestWindowProxyPlanner.plan(
      vmName: "Dev VM",
      window: window,
      maximumHostSize: GuestWindowProxyPlan.HostSize(width: 400, height: 300)
    )

    XCTAssertEqual(
      plan.displaydWindowCropArguments(
        framebufferSize: GuestWindowProxyPlan.FramebufferSize(width: 1440, height: 900),
        backingScale: 2,
        framebufferRGBAFile: "/tmp/framebuffer.rgba",
        windowCropRGBAFile: "/tmp/window-crop.rgba"
      ),
      [
        "--framebuffer-width",
        "1440",
        "--framebuffer-height",
        "900",
        "--scale",
        "2",
        "--window-id",
        "0x01200007",
        "--window-title",
        "Real Terminal",
        "--window-x",
        "30",
        "--window-y",
        "40",
        "--window-width",
        "800",
        "--window-height",
        "600",
        "--window-host-width",
        "400",
        "--window-host-height",
        "300",
        "--framebuffer-rgba-file",
        "/tmp/framebuffer.rgba",
        "--window-crop-rgba-file",
        "/tmp/window-crop.rgba",
      ]
    )
  }

  func testPlanCarriesCropFrameSummaryPathWhenGuestReportsIt() throws {
    let window = GuestToolsWindowAction(
      id: "0x01200007",
      title: "Real Terminal",
      source: "wmctrl",
      bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600),
      cropFrameSummaryPath: "/tmp/displayd-window-crop.json"
    )

    let plan = try GuestWindowProxyPlanner.plan(vmName: "Dev VM", window: window)

    XCTAssertEqual(plan.cropFrameSummaryPath, "/tmp/displayd-window-crop.json")
  }

  func testDisplaydWindowRegionArgumentsDefaultToMinimumFramebufferAndPositiveScale() throws {
    let window = GuestToolsWindowAction(
      id: "0x02000010",
      title: "  ",
      source: "wmctrl",
      bounds: GuestToolsWindowBounds(x: -20, y: 850, width: 100, height: 100)
    )
    let plan = try GuestWindowProxyPlanner.plan(vmName: "Dev VM", window: window)

    XCTAssertEqual(plan.minimumFramebufferSize, GuestWindowProxyPlan.FramebufferSize(width: 100, height: 950))
    XCTAssertEqual(
      plan.displaydWindowRegionArguments(backingScale: 0),
      [
        "--framebuffer-width",
        "100",
        "--framebuffer-height",
        "950",
        "--scale",
        "1",
        "--window-id",
        "0x02000010",
        "--window-x",
        "-20",
        "--window-y",
        "850",
        "--window-width",
        "100",
        "--window-height",
        "100",
        "--window-host-width",
        "100",
        "--window-host-height",
        "100",
      ]
    )
  }

  func testGuestPointMapsNativeHostCoordinatesIntoGuestBounds() throws {
    let window = GuestToolsWindowAction(
      id: "0x01200007",
      title: "Terminal",
      source: "wmctrl",
      bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600)
    )
    let plan = try GuestWindowProxyPlanner.plan(vmName: "Dev VM", window: window)

    XCTAssertEqual(
      plan.guestPoint(forHostPoint: GuestWindowProxyPlan.HostPoint(x: 10, y: 20)),
      GuestWindowProxyPlan.GuestPoint(x: 40, y: 60)
    )
  }

  func testGuestPointScalesAndClampsHostCoordinates() throws {
    let window = GuestToolsWindowAction(
      id: "0x02000010",
      title: "Large Window",
      source: "wmctrl",
      bounds: GuestToolsWindowBounds(x: 100, y: 200, width: 3000, height: 2000)
    )
    let plan = try GuestWindowProxyPlanner.plan(
      vmName: "Dev VM",
      window: window,
      maximumHostSize: GuestWindowProxyPlan.HostSize(width: 1500, height: 1000)
    )

    XCTAssertEqual(
      plan.guestPoint(forHostPoint: GuestWindowProxyPlan.HostPoint(x: 750, y: 500)),
      GuestWindowProxyPlan.GuestPoint(x: 1600, y: 1200)
    )
    XCTAssertEqual(
      plan.guestPoint(forHostPoint: GuestWindowProxyPlan.HostPoint(x: 2_000, y: 2_000)),
      GuestWindowProxyPlan.GuestPoint(x: 3099, y: 2199)
    )
    XCTAssertEqual(
      plan.guestPoint(forHostPoint: GuestWindowProxyPlan.HostPoint(x: -20, y: -30)),
      GuestWindowProxyPlan.GuestPoint(x: 100, y: 200)
    )
  }

  func testGuestBoundsMapHostMoveAndResizeAtNativeScale() throws {
    let window = GuestToolsWindowAction(
      id: "0x01200007",
      title: "Terminal",
      source: "wmctrl",
      bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600)
    )
    let plan = try GuestWindowProxyPlanner.plan(vmName: "Dev VM", window: window)

    let bounds = plan.guestBounds(
      forHostContentFrame: GuestWindowProxyPlan.HostFrame(
        x: 200,
        y: 50,
        width: 1024,
        height: 768
      ),
      relativeTo: GuestWindowProxyPlan.HostFrame(
        x: 100,
        y: 100,
        width: 800,
        height: 600
      )
    )

    XCTAssertEqual(bounds, GuestToolsWindowBounds(x: 130, y: 90, width: 1024, height: 768))
  }

  func testGuestBoundsMapHostMoveAndResizeThroughScaledProxy() throws {
    let window = GuestToolsWindowAction(
      id: "0x02000010",
      title: "Large Window",
      source: "wmctrl",
      bounds: GuestToolsWindowBounds(x: 100, y: 200, width: 3000, height: 2000)
    )
    let plan = try GuestWindowProxyPlanner.plan(
      vmName: "Dev VM",
      window: window,
      maximumHostSize: GuestWindowProxyPlan.HostSize(width: 1500, height: 1000)
    )

    let bounds = plan.guestBounds(
      forHostContentFrame: GuestWindowProxyPlan.HostFrame(
        x: 260,
        y: 400,
        width: 1200,
        height: 900
      ),
      relativeTo: GuestWindowProxyPlan.HostFrame(
        x: 10,
        y: 500,
        width: 1500,
        height: 1000
      )
    )

    XCTAssertEqual(bounds, GuestToolsWindowBounds(x: 600, y: 400, width: 2400, height: 1800))
  }

  func testGuestBoundsKeepPositiveSizeForTinyHostContentFrame() throws {
    let window = GuestToolsWindowAction(
      id: "0x01200007",
      title: "Terminal",
      source: "wmctrl",
      bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600)
    )
    let plan = try GuestWindowProxyPlanner.plan(vmName: "Dev VM", window: window)

    let bounds = plan.guestBounds(
      forHostContentFrame: GuestWindowProxyPlan.HostFrame(x: 100, y: 100, width: 0, height: 0),
      relativeTo: GuestWindowProxyPlan.HostFrame(x: 100, y: 100, width: 800, height: 600)
    )

    XCTAssertEqual(bounds, GuestToolsWindowBounds(x: 30, y: 40, width: 1, height: 1))
  }

  func testPlanRequiresWindowBounds() {
    let window = GuestToolsWindowAction(id: "window-1", title: "No Bounds")

    XCTAssertThrowsError(try GuestWindowProxyPlanner.plan(vmName: "Dev VM", window: window)) {
      error in
      XCTAssertEqual(error as? GuestWindowProxyPlanner.PlanningError, .missingBounds)
    }
  }
}
