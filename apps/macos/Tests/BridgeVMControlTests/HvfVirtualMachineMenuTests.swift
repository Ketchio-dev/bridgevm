#if canImport(AppKit)
import Foundation
import SwiftUI
import XCTest
@testable import BridgeVMControl

/// The menu bar's 가상 머신 menu replaces the Guest windows button that sat over the top-right corner
/// of the Windows display, where a maximized Windows app has its close button.
@MainActor
final class HvfVirtualMachineMenuTests: XCTestCase {
    private typealias Owned = HvfOwnedRuntimeControllerTests.Fixture

    /// Only the framebuffer, the notices and the paused overlay are over Windows; the Guest windows
    /// sheet hangs off the display, opened from the menu.
    func testTheDisplayHasNoButtonOverWindows() {
        let root = URL(fileURLWithPath: "/tmp/bridgevm-menu-" + UUID().uuidString)
        let session = HvfEngineSession(config: config(root), repoRoot: root) { _ in false }
        let body = String(reflecting: type(of: HvfLiveDisplaySurface(session: session).body))
        XCTAssertTrue(body.contains("HvfFramebufferView"), body)
        XCTAssertFalse(body.contains("Button"), body)
        XCTAssertTrue(body.contains("HvfWindowInventoryPanel"), body)
    }

    func testItemsFollowTheSessionInFront() throws {
        let off = HvfVirtualMachineMenuState(session: nil)
        XCTAssertEqual(off, HvfVirtualMachineMenuState(sendsCtrlAltDelete: false, pauses: false, paused: false,
                                                       restarts: false, stops: false, listsWindows: false))
        let root = URL(fileURLWithPath: "/tmp/bridgevm-menu-" + UUID().uuidString)
        XCTAssertEqual(HvfVirtualMachineMenuState(session: HvfEngineSession(config: config(root), repoRoot: root) { _ in false }),
                       off, "a stopped VM's window")

        let f = try Owned(); defer { f.clean() }
        let session = bind(f)
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        session.beginOwnedInputBoot()
        session.connectionState = .booting
        var state = HvfVirtualMachineMenuState(session: session)
        XCTAssertTrue(state.sendsCtrlAltDelete && state.pauses && state.restarts && state.stops)
        XCTAssertFalse(state.paused)
        XCTAssertFalse(state.listsWindows, "no agent yet to list windows")

        session.connectionState = .connected(host: "BRIDGEVM")
        XCTAssertTrue(HvfVirtualMachineMenuState(session: session).listsWindows)

        session.pause = .paused
        state = HvfVirtualMachineMenuState(session: session)
        XCTAssertTrue(state.pauses && state.paused, "재개")
        XCTAssertFalse(state.sendsCtrlAltDelete || state.listsWindows, "a paused guest takes no input")
        session.pause = .running

        XCTAssertEqual(session.stopOwned(expectedToken: f.controller.identity.token), .requested(deadline: nil))
        state = HvfVirtualMachineMenuState(session: session)
        XCTAssertFalse(state.stops, "the stop is under way")
    }

    func testTheMenuActsOnTheMainVMWindow() {
        let root = URL(fileURLWithPath: "/tmp/bridgevm-menu-" + UUID().uuidString)
        let target = HvfVirtualMachineMenuTarget()
        let a = HvfDisplayWindowState(session: HvfEngineSession(config: config(root), repoRoot: root) { _ in false })
        let b = HvfDisplayWindowState(session: HvfEngineSession(config: config(root), repoRoot: root) { _ in false })
        target.became(a)
        target.resigned(b)
        XCTAssertTrue(target.display === a)
        target.became(b)
        target.resigned(a)
        XCTAssertTrue(target.display === b)
        target.resigned(b)
        XCTAssertNil(target.display, "the library window is in front")

        a.perform(.windows)
        XCTAssertTrue(a.showsWindowInventory, "Guest windows… opens the window's sheet")
        XCTAssertFalse(b.showsWindowInventory)
    }

    private func config(_ root: URL) -> HvfEngineConfig {
        HvfEngineConfig(targetDiskPath: root.appendingPathComponent("disk").path,
            uefiVarsPath: root.appendingPathComponent("vars").path, evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true, shareHostDir: nil, shareGuestDir: nil,
            virtioNet: true, virtioGpu3d: false, nvmeBufferedIO: true,
            ctlFilePath: root.appendingPathComponent("agent.ctl").path)
    }

    private func bind(_ f: Owned) -> HvfEngineSession {
        let session = HvfEngineSession(config: f.base.config, repoRoot: f.base.root,
            processIsRunning: { _ in false }, vtpmKeyProvider: f.base.keys)
        session.process = f.process
        session.ownedProcessIdentity = f.controller.identity
        session.ownedController = f.controller
        session.ownedStartNow = { f.clock.value }
        let token = f.controller.identity.token
        f.controller.onChange = { [weak session] in session?.ownedRuntimeChanged(token: token) }
        try? FileManager.default.createDirectory(atPath: f.base.config.evidenceDir, withIntermediateDirectories: true)
        return session
    }
}
#endif
