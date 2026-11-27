#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfDisplaySurfaceAccessibilityTests: XCTestCase {
    func testValueReportsOnlyAPresentedFrame() {
        let waiting = "Waiting for Windows to show a picture"
        XCTAssertEqual(HvfDisplaySurfaceAccessibility.value(.zero), waiting)
        XCTAssertEqual(HvfDisplaySurfaceAccessibility.value(CGSize(width: 1920, height: 0)), waiting)
        XCTAssertEqual(HvfDisplaySurfaceAccessibility.value(CGSize(width: 1920, height: 1080)), "1920 by 1080")
    }

    func testFramebufferViewStartsWithoutAFrame() {
        let view = makeView()
        defer { view.teardown() }
        XCTAssertTrue(view.isAccessibilityElement())
        XCTAssertEqual(view.accessibilityRole(), .image)
        XCTAssertEqual(view.accessibilityLabel(), "Windows display")
        XCTAssertEqual(view.accessibilityIdentifier(), "bridgevm.runtime.display.surface")
        XCTAssertEqual(view.accessibilityValue() as? String, "Waiting for Windows to show a picture")
    }

    /// VoiceOver reads the help aloud, so the press count is only for the end-to-end harness.
    func testAPressOutsideTheEndToEndHarnessLeavesTheHelpEmpty() throws {
        let view = makeView()
        defer { view.teardown() }
        view.mouseDown(with: try XCTUnwrap(NSEvent.mouseEvent(
            with: .leftMouseDown, location: CGPoint(x: 10, y: 10), modifierFlags: [], timestamp: 0,
            windowNumber: 0, context: nil, eventNumber: 0, clickCount: 1, pressure: 1)))
        XCTAssertNil(view.accessibilityHelp())
    }

    func testTheEndToEndHarnessReadsWhereThePressWent() {
        let view = NSView()
        HvfDisplaySurfaceAccessibility.notePress(view, count: 2, outcome: "no-guest-size", reports: true)
        XCTAssertEqual(view.accessibilityHelp(), "presses=2 last=no-guest-size")
    }

    private func makeView() -> FBLayerView {
        let root = URL(fileURLWithPath: "/tmp/bridgevm-surface-" + UUID().uuidString)
        let config = HvfEngineConfig(
            targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: true, virtioGpu3d: false,
            nvmeBufferedIO: true, ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        return FBLayerView(session: HvfEngineSession(config: config, repoRoot: root) { _ in true })
    }
}
#endif
