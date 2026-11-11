#if canImport(AppKit)
import AppKit
import SwiftUI

/// One VM display window (HvfDisplayWindowController): its session, its Guest windows sheet, and its Windows apps
/// (the 가상 머신 > Windows 앱 submenu and its 모든 앱… sheet).
@MainActor
final class HvfDisplayWindowState: ObservableObject {
    let session: HvfEngineSession
    let startApps: HvfGuestStartAppsController
    weak var window: NSWindow?
    @Published var showsWindowInventory = false
    @Published var showsAllApps = false

    init(session: HvfEngineSession) {
        self.session = session
        startApps = HvfGuestStartAppsController(session: session)
        startApps.onNotice = { [weak self] in self?.tell($0) }
    }
}

/// The VM display window that is the main window, which the menu bar's 가상 머신 menu acts on;
/// none while the library window is in front.
@MainActor
final class HvfVirtualMachineMenuTarget: ObservableObject {
    static let shared = HvfVirtualMachineMenuTarget()
    @Published private(set) var display: HvfDisplayWindowState?
    private var menuBarTracking: NSObjectProtocol?

    /// The menu bar opening asks the guest for its apps again when the list is stale, so Windows 앱 is current
    /// (NSMenu.didBeginTrackingNotification's object is the main menu for the menu bar).
    init() {
        menuBarTracking = NotificationCenter.default.addObserver(forName: NSMenu.didBeginTrackingNotification,
                                                                 object: nil, queue: .main) { [weak self] note in
            MainActor.assumeIsolated {
                guard let self, let menu = note.object as? NSMenu, menu === NSApp.mainMenu else { return }
                self.menuBarOpened()
            }
        }
    }

    deinit { if let menuBarTracking { NotificationCenter.default.removeObserver(menuBarTracking) } }

    func menuBarOpened() {
        guard let display, display.startApps.unavailableReason == nil else { return }
        display.startApps.loadIfStale()
    }

    func became(_ display: HvfDisplayWindowState) { if self.display !== display { self.display = display } }
    func resigned(_ display: HvfDisplayWindowState) { if self.display === display { self.display = nil } }
}

/// Which items the session in front takes now, by the rules of the dashboard's own controls; all
/// off without a VM window in front.
struct HvfVirtualMachineMenuState: Equatable {
    var sendsKeys = false
    var pauses = false
    var paused = false
    var restarts = false
    var stops = false
    var listsWindows = false
    /// Why Windows 앱 lists no app, in one plain sentence; nil when the guest takes STARTAPPS now.
    var windowsAppsUnavailable: String? = HvfGuestStartApps.notRunning
}

extension HvfVirtualMachineMenuState {
    @MainActor init(session: HvfEngineSession?) {
        self.init()
        guard let session else { return }
        let running = session.vmmControlAvailable && !session.runtimeStartupWorkerPending
        sendsKeys = running && !session.pause.holdsInput
        paused = session.pause.showsResume
        pauses = paused || session.pauseAvailable // HvfPauseControl
        restarts = running // HvfRestartControl
        // The dashboard's 정지, as a quit asks it: this app's own run, with no stop under way.
        stops = session.mayHaveOwnedWork && session.terminationStopAvailable && !session.runtimeStartupWorkerPending
        if case .connected = session.connectionState { listsWindows = !session.pause.holdsInput }
        windowsAppsUnavailable = HvfGuestStartApps.unavailableReason(session)
    }
}

enum HvfVirtualMachineMenuAction {
    case ctrlAltDelete, screenCapture, pause, restart, stop, windows
    case startApp(HvfGuestStartApp), allApps, refreshApps
}

extension HvfDisplayWindowState {
    func perform(_ action: HvfVirtualMachineMenuAction) {
        switch action {
        case .ctrlAltDelete:
            if !session.sendToolbarKey("ctrl+alt+delete") { tell("Windows에 Ctrl+Alt+Delete를 보내지 못했습니다. 잠시 뒤 다시 시도하세요.") }
        case .screenCapture:
            // Windows+Shift+S opens the Snipping Tool's capture bar, whatever the Command key setting.
            if !session.sendToolbarKey("win+shift+s") { tell("Windows에 화면 캡처 키(Windows+Shift+S)를 보내지 못했습니다. 잠시 뒤 다시 시도하세요.") }
        case .pause:
            tell(session.togglePause().message)
        case .restart:
            switch session.requestRestart() {
            case .requested: break
            case let .refused(message): tell(message)
            case let .needsResetConfirmation(prompt): confirmReset(prompt)
            }
        case .stop:
            session.requestTerminationStop()
        case .windows:
            showsWindowInventory = true
        case let .startApp(app):
            if !startApps.launch(app) { tell(HvfGuestStartApps.notSent) }
        case .allApps:
            showsAllApps = true
        case .refreshApps:
            startApps.refresh()
        }
    }

    /// The dashboard's Reset confirmation (HvfRestartControl), as a sheet on the VM window.
    private func confirmReset(_ prompt: HvfResetPrompt) {
        let alert = NSAlert()
        alert.messageText = HvfRestartControl<EmptyView>.title(prompt)
        alert.informativeText = HvfRestartControl<EmptyView>.message(prompt)
        alert.addButton(withTitle: "취소")
        alert.addButton(withTitle: "재설정").hasDestructiveAction = true
        if prompt == .windowsHasNotRestarted { alert.addButton(withTitle: "다시 요청") }
        present(alert) { [self] response in
            switch response {
            case .alertSecondButtonReturn: tell(session.requestReset().message)
            case .alertThirdButtonReturn: tell(session.requestWindowsRestart().message)
            default: session.cancelResetConfirmation()
            }
        }
    }

    private func tell(_ message: String?) {
        guard let message else { return }
        let alert = NSAlert()
        alert.messageText = message
        alert.addButton(withTitle: "확인")
        present(alert) { _ in }
    }

    private func present(_ alert: NSAlert, then: @escaping @MainActor (NSApplication.ModalResponse) -> Void) {
        guard let window, window.isVisible else { then(alert.runModal()); return }
        alert.beginSheetModal(for: window) { response in MainActor.assumeIsolated { then(response) } }
    }
}

/// The menu bar's 가상 머신 menu. No key equivalents: every key in the VM window goes to Windows.
struct HvfVirtualMachineCommands: Commands {
    var body: some Commands {
        CommandMenu("가상 머신") { HvfVirtualMachineMenuContent() }
    }
}

/// SwiftUI rebuilds a menu bar menu from its views as the menu opens, so the items follow the
/// window in front and its session's state at that moment.
private struct HvfVirtualMachineMenuContent: View {
    @ObservedObject var target = HvfVirtualMachineMenuTarget.shared

    var body: some View {
        if let display = target.display {
            HvfVirtualMachineSessionMenuItems(display: display, session: display.session)
        } else {
            HvfVirtualMachineMenuItems(state: HvfVirtualMachineMenuState(session: nil), startApps: nil) { _ in }
        }
    }
}

private struct HvfVirtualMachineSessionMenuItems: View {
    let display: HvfDisplayWindowState
    @ObservedObject var session: HvfEngineSession

    var body: some View {
        HvfVirtualMachineMenuItems(state: HvfVirtualMachineMenuState(session: session), startApps: display.startApps) {
            display.perform($0)
        }
    }
}

struct HvfVirtualMachineMenuItems: View {
    let state: HvfVirtualMachineMenuState
    let startApps: HvfGuestStartAppsController?
    let perform: @MainActor (HvfVirtualMachineMenuAction) -> Void

    var body: some View {
        Button("Ctrl+Alt+Delete 보내기") { perform(.ctrlAltDelete) }.disabled(!state.sendsKeys)
        Button("화면 캡처 (Snipping Tool)") { perform(.screenCapture) }.disabled(!state.sendsKeys)
        Divider()
        Button(HvfPauseControl<EmptyView>.title(paused: state.paused)) { perform(.pause) }.disabled(!state.pauses)
        Button("다시 시작") { perform(.restart) }.disabled(!state.restarts)
        Button("정지") { perform(.stop) }.disabled(!state.stops)
        Divider()
        Menu("Windows 앱") {
            if let reason = state.windowsAppsUnavailable {
                Button(reason) {}.disabled(true)
            } else if let startApps {
                HvfGuestStartAppsMenuItems(apps: startApps, perform: perform)
            }
        }
        .accessibilityIdentifier("runtime.windowsApps")
        Button("Guest windows…") { perform(.windows) }.disabled(!state.listsWindows)
            .accessibilityIdentifier("runtime.windows.open")
    }
}

/// Windows 앱's items: up to HvfGuestStartApps.menuLimit apps, alphabetically, or what the list is doing.
struct HvfGuestStartAppsMenuItems: View {
    @ObservedObject var apps: HvfGuestStartAppsController
    let perform: @MainActor (HvfVirtualMachineMenuAction) -> Void

    var body: some View {
        let shown = apps.menuApps
        if shown.isEmpty { Button(apps.statusText) {}.disabled(true) }
        ForEach(shown) { app in Button(app.name) { perform(.startApp(app)) } }
        Divider()
        Button("모든 앱…") { perform(.allApps) }
        Button("목록 새로 고침") { perform(.refreshApps) }.disabled(apps.status == .loading)
    }
}
#endif
