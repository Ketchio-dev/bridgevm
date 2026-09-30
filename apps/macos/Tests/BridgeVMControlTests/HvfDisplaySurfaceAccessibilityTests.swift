#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfDisplaySurfaceAccessibilityTests: XCTestCase {
    func testValueReportsOnlyAPresentedFrame() {
        XCTAssertEqual(HvfDisplaySurfaceAccessibility.value(.zero), "no-frame")
        XCTAssertEqual(HvfDisplaySurfaceAccessibility.value(CGSize(width: 1920, height: 0)), "no-frame")
        XCTAssertEqual(HvfDisplaySurfaceAccessibility.value(CGSize(width: 1920, height: 1080)), "frame 1920x1080")
    }

    func testFramebufferViewStartsWithoutAFrame() {
        let root = URL(fileURLWithPath: "/tmp/bridgevm-surface-" + UUID().uuidString)
        let config = HvfEngineConfig(
            targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: true, virtioGpu3d: false,
            nvmeBufferedIO: true, ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        let view = FBLayerView(session: HvfEngineSession(config: config, repoRoot: root) { _ in true })
        defer { view.teardown() }
        XCTAssertTrue(view.isAccessibilityElement())
        XCTAssertEqual(view.accessibilityRole(), .image)
        XCTAssertEqual(view.accessibilityIdentifier(), "bridgevm.runtime.display.surface")
        XCTAssertEqual(view.accessibilityValue() as? String, "no-frame")
    }
}
#endif
