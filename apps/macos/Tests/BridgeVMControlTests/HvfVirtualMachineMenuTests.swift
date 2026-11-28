#if canImport(AppKit)
import Foundation
import SwiftUI
import XCTest
@testable import BridgeVMControl

/// The menu bar's Virtual Machine menu replaces the Guest windows button that sat over the top-right corner
/// of the Windows display, where a maximized Windows app has its close button.
@MainActor
final class HvfVirtualMachineMenuTests: XCTestCase {
    private typealias Owned = HvfOwnedRuntimeControllerTests.Fixture

    func testNativeMenuShowsEnglishActionsAndTheStoppedGuestExplanation() throws {
        guard #available(macOS 14.4, *) else { throw XCTSkip("Native SwiftUI menu hosting requires macOS 14.4") }
        let menu = NSHostingMenu(rootView: HvfVirtualMachineMenuContent())
        menu.update()
        XCTAssertEqual(menu.items.filter { !$0.isSeparatorItem }.map(\.title), [
            "Settings…", "Send Ctrl+Alt+Delete", "Open Snipping Tool", "Pause", "Restart", "Stop",
            "Windows Apps", "Show Open Windows…"
        ])
        let settings = try XCTUnwrap(menu.items.first)
        XCTAssertEqual(settings.keyEquivalent, ",")
        XCTAssertEqual(settings.keyEquivalentModifierMask.intersection(.deviceIndependentFlagsMask), .command)
        let apps = try XCTUnwrap(menu.items.first { $0.title == "Windows Apps" }?.submenu)
        apps.update()
        XCTAssertEqual(apps.items.map(\.title), ["Start the VM to see its Windows apps."])
        XCTAssertFalse(try XCTUnwrap(apps.items.first).isEnabled)
    }

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
        XCTAssertEqual(off, HvfVirtualMachineMenuState(sendsKeys: false, pauses: false, paused: false,
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
        XCTAssertTrue(state.sendsKeys && state.pauses && state.restarts && state.stops)
        XCTAssertFalse(state.paused)
        XCTAssertFalse(state.listsWindows, "no agent yet to list windows")

        session.connectionState = .connected(host: "BRIDGEVM")
        XCTAssertTrue(HvfVirtualMachineMenuState(session: session).listsWindows)

        session.pause = .paused
        state = HvfVirtualMachineMenuState(session: session)
        XCTAssertTrue(state.pauses && state.paused, "Resume")
        XCTAssertFalse(state.sendsKeys || state.listsWindows, "a paused guest takes no input")
        session.pause = .running

        XCTAssertEqual(session.stopOwned(expectedToken: f.controller.identity.token), .requested(deadline: nil))
        state = HvfVirtualMachineMenuState(session: session)
        XCTAssertFalse(state.stops, "the stop is under way")
    }

    /// Once a Stop waits for Windows the menu's Stop is off, so the display itself shows the wait with the
    /// library's Shut Down Again and Force Stop; a full-screen VM window had no sign of it and no way out.
    func testTheDisplayShowsAStopWaitingForWindowsWithItsWayOut() async throws {
        let f = try Owned(); defer { f.clean() }
        let session = bind(f)
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        // The strip's close button is .plain, so the probe sees only Force Stop and Shut Down Again.
        let buttons = InstallCancellationButtonProbe()
        let host = NSHostingView(rootView: HvfLiveDisplaySurface(session: session)
            .buttonStyle(InstallCancellationProbeStyle(probe: buttons)))
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1280, height: 800),
                              styleMask: [], backing: .buffered, defer: true)
        window.contentView = host
        defer { window.contentView = nil }
        func roles() async -> [ButtonRole?] {
            for _ in 0..<5 { await Task.yield() }
            host.layoutSubtreeIfNeeded()
            return buttons.entries.values.map(\.role)
        }
        let none = await roles()
        XCTAssertEqual(none, [], "no stop yet")

        XCTAssertEqual(session.stopOwned(expectedToken: f.controller.identity.token), .requested(deadline: nil))
        let waiting = await roles()
        XCTAssertEqual(waiting, [.destructive], "Force Stop while Windows closes its apps")

        f.clock.value = 290; f.controller.tick()
        XCTAssertEqual(session.stopWait, .windowsStillRunning)
        let stillRunning = await roles()
        XCTAssertEqual(stillRunning.count, 2, "Shut Down Again and Force Stop")
        try XCTUnwrap(buttons.entries.values.first { $0.role == nil }).activateIfEnabled()
        XCTAssertEqual(session.stopWait, .windowsClosingApps, "the window's Shut Down Again asked Windows again")
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
        XCTAssertTrue(a.showsWindowInventory, "Show Open Windows… opens the window's sheet")
        XCTAssertFalse(b.showsWindowInventory)
    }

    /// Open Snipping Tool sends Windows+Shift+S, the capture bar's own chord, in both Command key
    /// settings: a Mac keyboard has no PrtSc key and ⌘⇧S is Ctrl+Shift+S in the default one.
    func testScreenCaptureSendsWindowsShiftSInBothCommandKeyModes() throws {
        for mode in [HvfCommandKeyMode.mac, .windows] {
            let root = URL(fileURLWithPath: "/tmp/bridgevm-menu-" + UUID().uuidString)
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            defer { try? FileManager.default.removeItem(at: root) }
            var config = config(root)
            config.commandKeyMode = mode
            HvfDisplayWindowState(session: HvfEngineSession(config: config, repoRoot: root) { _ in false }).perform(.screenCapture)
            XCTAssertEqual(try String(contentsOf: root.appendingPathComponent("input.ctl"), encoding: .utf8),
                           "KEY win+shift+s\n", mode.rawValue)
        }
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
