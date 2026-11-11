#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

/// A VM's display window opens where that VM's window was last, and in full screen if it was; the
/// guest's first RESIZE then asks for that window's content size. Windows are made, never shown.
@MainActor
final class HvfDisplayWindowPlacementTests: XCTestCase {
    private let moved = NSRect(x: 60, y: 120, width: 1000, height: 660)

    func testAReopenedWindowGetsItsVMsSavedFrameAndAnotherVMsDoesNot() throws {
        let a = config(), b = config()
        defer { forget(a, b) }
        let first = controller(a)
        let window = try XCTUnwrap(first.window)
        let opened = window.frame
        XCTAssertEqual(window.contentRect(forFrameRect: opened).size, NSSize(width: 1280, height: 800), "nothing saved yet")
        window.setFrame(moved, display: false)
        window.close()

        let reopened = controller(a), other = controller(b)
        XCTAssertEqual(reopened.window?.frame, moved, "the same VM's window opens where it was")
        XCTAssertEqual(other.window?.frame, opened, "another VM's opens 1280x800 centred")
        reopened.close()
        other.close()
    }

    func testFullScreenIsKeptPerVMAndItsFrameIsNeverSaved() throws {
        let a = config(), b = config()
        defer { forget(a, b) }
        let first = controller(a)
        let window = try XCTUnwrap(first.window)
        window.setFrame(moved, display: false)
        post(NSWindow.willEnterFullScreenNotification, window)
        window.setFrame(NSRect(x: 0, y: 0, width: 1728, height: 1117), display: false) // full screen's frame
        post(NSWindow.didEnterFullScreenNotification, window)
        XCTAssertTrue(HvfDisplayWindowPlacement(config: a).fullScreen)
        XCTAssertFalse(HvfDisplayWindowPlacement(config: b).fullScreen)
        window.close()
        post(NSWindow.didExitFullScreenNotification, window) // as AppKit may, once the window closed

        let second = controller(a)
        let reopened = try XCTUnwrap(second.window)
        XCTAssertTrue(second.placement.fullScreen, "closed in full screen, it reopens in full screen")
        XCTAssertEqual(reopened.frame, moved, "at the window's own frame, for when it leaves full screen")
        post(NSWindow.willEnterFullScreenNotification, reopened)
        post(NSWindow.didEnterFullScreenNotification, reopened)
        post(NSWindow.didExitFullScreenNotification, reopened)
        XCTAssertFalse(second.placement.fullScreen, "left full screen")
        reopened.close()
        XCTAssertFalse(controller(a).placement.fullScreen)
    }

    /// The display view fills the window's content, and only a shown view reports its size
    /// (FBLayerView.reportDisplaySize): the first RESIZE is the restored content size, not 1280x800.
    func testTheFirstResizeAsksForTheRestoredContentSize() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        let vm = config()
        defer { forget(vm) }
        let first = controller(vm)
        first.window?.setFrame(moved, display: false)
        first.close()
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session)
            session.config.libraryContext = vm.libraryContext
            try autoreleasepool {
                let display = HvfDisplayWindowController(session: session, title: "vm")
                let window = try XCTUnwrap(display.window)
                let content = try XCTUnwrap(window.contentView).frame.size
                XCTAssertEqual(content, window.contentRect(forFrameRect: moved).size)
                XCTAssertNotEqual(content, NSSize(width: 1280, height: 800))
                session.displayViewResized(to: content, scale: window.backingScaleFactor, settled: true)
                XCTAssertEqual(try fixture.bytes(fixture.input),
                               Data("RESIZE \(Int(content.width))x\(Int(content.height))\n".utf8))
                display.close()
            }
        }
    }

    private func controller(_ config: HvfEngineConfig) -> HvfDisplayWindowController {
        HvfDisplayWindowController(session: session(config), title: "vm")
    }

    private func post(_ name: Notification.Name, _ window: NSWindow) {
        NotificationCenter.default.post(name: name, object: window)
    }

    private func session(_ config: HvfEngineConfig) -> HvfEngineSession {
        HvfEngineSession(config: config, repoRoot: URL(fileURLWithPath: config.evidenceDir)) { _ in false }
    }

    /// A library VM with an id of its own, so the app's defaults keep nothing of another test's.
    private func config() -> HvfEngineConfig {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("window-frame-" + UUID().uuidString)
        let id = "window-frame-test-" + UUID().uuidString
        let vm = VMConfig(id: id, name: id, displayName: id, backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: root.appendingPathComponent("bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: id, displayWidth: 1280, displayHeight: 800)
        return HvfEngineConfig(targetDiskPath: root.appendingPathComponent("disk").path,
            uefiVarsPath: root.appendingPathComponent("vars").path, evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true, shareHostDir: nil, shareGuestDir: nil,
            virtioNet: true, virtioGpu3d: false, nvmeBufferedIO: true,
            ctlFilePath: root.appendingPathComponent("agent.ctl").path,
            libraryContext: HvfLibraryLaunchContext(config: vm, rootURL: root))
    }

    private func forget(_ configs: HvfEngineConfig...) {
        for config in configs {
            let placement = HvfDisplayWindowPlacement(config: config)
            NSWindow.removeFrame(usingName: placement.frameName)
            UserDefaults.standard.removeObject(forKey: placement.fullScreenKey)
        }
    }
}
#endif
