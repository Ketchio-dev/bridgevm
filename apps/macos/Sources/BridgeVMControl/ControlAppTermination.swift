#if canImport(AppKit)
import AppKit

/// One VM run whose runner the app's exit would cancel (its owner input closes).
@MainActor
protocol ControlAppTerminationSession: AnyObject {
    /// True until the run's teardown is confirmed, or while a start may still spawn a runner.
    var blocksAppTermination: Bool { get }
    var isInstallation: Bool { get }
    var requiresTerminationWait: Bool { get }
    /// The user's Stop; repeating it joins the stop already admitted.
    func requestTerminationStop()
    /// Force Stop for a stop that is still waiting on Windows.
    func requestTerminationForceStop()
    /// Reads the run's progress where its own default-mode timer cannot fire.
    func pollTerminationWait()
}

extension ControlAppTerminationSession {
    var isInstallation: Bool { false }
    var requiresTerminationWait: Bool { false }
}

/// Quitting (Cmd+Q, the Dock, the last window, or a Mac logout, restart or shutdown) stops each
/// running Windows like the Stop button first and exits only after every run's teardown is confirmed,
/// so the library owner lock is released after the stops (`applicationWillTerminate`).
@MainActor
final class ControlAppTermination {
    enum QuitReason: Equatable, Sendable {
        case user
        /// The system asked: no question first, the stops simply run.
        case logoutRestartOrShutdown
    }

    enum Plan: Equatable, Sendable { case terminateNow, confirmThenStop, stopWithoutConfirmation }

    enum DeadlineChoice: Equatable, Sendable { case forceStop, keepWaiting, cancelQuit, quitAnyway }

    struct Presenter {
        /// The one question before any stop: true quits after Windows shuts down.
        var confirmQuit: @MainActor (_ runningCount: Int) -> Bool
        /// Asked each time Windows has not turned off by the deadline. `afterForce` offers quitting anyway.
        var decideAtDeadline: @MainActor (_ afterForce: Bool, _ stillRunning: @escaping @MainActor () -> Bool) -> DeadlineChoice
        var confirmWithInstallations: (@MainActor (_ runningCount: Int, _ installCount: Int) -> Bool)? = nil
        var confirmAfterPreparation: @MainActor () -> Bool = { false }
        var revealLibrary: @MainActor () -> Void = {}
    }

    static let deadline: TimeInterval = 60

    static func plan(blockingSessions: Int, reason: QuitReason) -> Plan {
        guard blockingSessions > 0 else { return .terminateNow }
        return reason == .user ? .confirmThenStop : .stopWithoutConfirmation
    }

    private let sessions: @MainActor () -> [ControlAppTerminationSession]
    private let presenter: Presenter
    private let reply: @MainActor (Bool) -> Void
    private let now: () -> TimeInterval
    private let preparationLibrary: @MainActor () -> LibraryModel?
    private let retryQuit: @MainActor () -> Void
    private var pending = false
    private var forced = false
    private var deciding = false
    private var deadlineAt: TimeInterval = 0
    private var timer: Timer?

    init(sessions: @escaping @MainActor () -> [ControlAppTerminationSession], presenter: Presenter,
         reply: @escaping @MainActor (Bool) -> Void,
         now: @escaping () -> TimeInterval = { ProcessInfo.processInfo.systemUptime },
         preparationLibrary: @escaping @MainActor () -> LibraryModel? = { nil },
         retryQuit: @escaping @MainActor () -> Void = {}) {
        self.sessions = sessions; self.presenter = presenter; self.reply = reply; self.now = now
        self.preparationLibrary = preparationLibrary; self.retryQuit = retryQuit
    }

    func shouldTerminate(reason: QuitReason) -> NSApplication.TerminateReply {
        guard !pending else { return .terminateCancel } // the pending quit answers for itself
        if let library = preparationLibrary(), library.vmPreparationInProgress {
            if reason == .user { finishPreparationBeforeQuit(library) }
            return .terminateCancel
        }
        let running = blocking()
        switch Self.plan(blockingSessions: running.count, reason: reason) {
        case .terminateNow: return .terminateNow
        case .confirmThenStop:
            let installs = running.filter(\.isInstallation).count
            let confirmed = installs > 0 ? (presenter.confirmWithInstallations?(running.count - installs, installs)
                ?? presenter.confirmQuit(running.count)) : presenter.confirmQuit(running.count)
            guard confirmed else { return .terminateCancel }
        case .stopWithoutConfirmation: break
        }
        pending = true; forced = false
        deadlineAt = now() + Self.deadline
        running.forEach { $0.requestTerminationStop() }
        startPolling()
        return .terminateLater
    }

    private func finishPreparationBeforeQuit(_ library: LibraryModel) {
        if library.quitAfterVMPreparation == nil {
            let retry = { [weak self] in DispatchQueue.main.async { self?.retryQuit() } }
            // runModal can deliver the worker's completion before the user answers.
            // Remember success during that interval. A failure clears the intent.
            var finished = false
            library.quitAfterVMPreparation = VMPreparationQuit(confirmed: false) { finished = true }
            guard presenter.confirmAfterPreparation() else {
                library.quitAfterVMPreparation = nil
                return
            }
            if finished { retry(); return }
            guard library.quitAfterVMPreparation != nil else { return }
            library.quitAfterVMPreparation = VMPreparationQuit(confirmed: true, onSuccess: retry)
        }
        if library.firstRunImportBusy && !library.preparingNewVM {
            library.selectedID = LibraryModel.firstRunImportSelectionID
        }
        presenter.revealLibrary()
    }

    /// One wait step: re-requests stops a pending start could not take yet, answers AppKit once
    /// every run is down, and asks the user at the deadline.
    func poll() {
        guard pending, !deciding else { return }
        let running = blocking()
        guard !running.isEmpty else { finish(true); return }
        running.forEach { $0.requestTerminationStop() }
        // Publication cannot be interrupted; pre-commit cancellation keeps the normal quit choices.
        guard !running.contains(where: \.requiresTerminationWait), now() >= deadlineAt else { return }
        deciding = true
        let choice = presenter.decideAtDeadline(forced) { [weak self] in
            guard let self else { return false }
            self.blocking().forEach { $0.pollTerminationWait() }
            return !self.blocking().isEmpty
        }
        deciding = false
        guard pending else { return }
        switch choice {
        case .forceStop:
            forced = true
            blocking().forEach { $0.requestTerminationForceStop() }
        case .keepWaiting: break
        case .cancelQuit: finish(false); return
        case .quitAnyway: finish(true); return
        }
        deadlineAt = now() + Self.deadline
        if blocking().isEmpty { finish(true) }
    }

    private func blocking() -> [ControlAppTerminationSession] { sessions().filter(\.blocksAppTermination) }

    private func finish(_ quit: Bool) {
        pending = false
        timer?.invalidate(); timer = nil
        reply(quit)
    }

    /// `terminateLater` and alerts run the main run loop in the modal panel mode only, where the
    /// sessions' own default-mode polling timers do not fire; this one polls them there.
    private func startPolling() {
        let timer = Timer(timeInterval: 0.25, repeats: true) { [weak self] _ in
            // A run-loop timer on the main run loop: its callout runs on the main thread, outside any
            // queued main-actor job, so the deadline question below can run its own modal loop.
            MainActor.assumeIsolated {
                guard let self else { return }
                self.blocking().forEach { $0.pollTerminationWait() }
                self.poll()
            }
        }
        RunLoop.main.add(timer, forMode: .modalPanel)
        self.timer = timer
    }
}

extension ControlAppTermination.QuitReason {
    /// The quit Apple Event's `kAEQuitReason` names a logout, restart or shutdown (or a system
    /// "quit all"); Cmd+Q, the Dock's Quit and the last window carry no reason.
    static func current(_ event: NSAppleEventDescriptor? = NSAppleEventManager.shared().currentAppleEvent) -> Self {
        guard let event, event.eventClass == AEEventClass(kCoreEventClass),
              event.eventID == AEEventID(kAEQuitApplication),
              let why = event.attributeDescriptor(forKeyword: AEKeyword(kAEQuitReason))
                ?? event.paramDescriptor(forKeyword: AEKeyword(kAEQuitReason)) else { return .user }
        let reason = why.typeCodeValue != 0 ? why.typeCodeValue : why.enumCodeValue
        let system: [OSType] = [kAEQuitAll, kAELogOut, kAEReallyLogOut, kAEShowRestartDialog, kAERestart,
                                kAEShowShutdownDialog, kAEShutDown].map { OSType($0) }
        return system.contains(reason) ? .logoutRestartOrShutdown : .user
    }
}

extension LibraryModel {
    var vmPreparationInProgress: Bool {
        preparingNewVM || firstRunImportBusy || appRuntimeSessions().contains(where: \.preparingSnapshot)
    }

    func finishVMPreparation(succeeded: Bool) {
        guard let request = quitAfterVMPreparation else { return }
        if succeeded && vmPreparationInProgress { return }
        quitAfterVMPreparation = nil
        if succeeded { request.onSuccess() }
    }
}

extension ControlAppTermination.Presenter {
    static let alerts = Self(
        confirmQuit: { count in
            let alert = NSAlert()
            alert.messageText = "Shut down Windows and quit BridgeVM?"
            let target = count == 1 ? "the running VM" : "all \(count) running VMs"
            alert.informativeText = "BridgeVM will ask Windows to shut down in \(target) and quit when shutdown is complete. Windows apps may ask you to save your work."
            alert.addButton(withTitle: "Shut Down and Quit")
            alert.addButton(withTitle: "Cancel")
            return alert.runModal() == .alertFirstButtonReturn
        },
        decideAtDeadline: { afterForce, stillRunning in
            let alert = shutdownDelayAlert(afterForce: afterForce)
            // Every run may still stop while the question is open: then the quit goes on by itself.
            let watcher = Timer(timeInterval: 0.25, repeats: true) { _ in
                MainActor.assumeIsolated { if !stillRunning() { NSApp.abortModal() } }
            }
            RunLoop.main.add(watcher, forMode: .modalPanel)
            defer { watcher.invalidate() }
            switch alert.runModal() {
            case .alertSecondButtonReturn: return afterForce ? .quitAnyway : .forceStop
            case .alertThirdButtonReturn: return .cancelQuit
            default: return .keepWaiting
            }
        },
        confirmWithInstallations: { running, installs in
            let alert = NSAlert()
            alert.messageText = "Cancel Windows installation and quit BridgeVM?"
            let target = installs == 1 ? "the installation" : "all \(installs) installations"
            alert.informativeText = "BridgeVM will cancel \(target) and wait for cleanup. Any installation already saving its result will finish first."
            if running > 0 {
                let machines = running == 1 ? "the running VM" : "all \(running) running VMs"
                alert.informativeText += " Windows will also shut down in \(machines)."
            }
            alert.addButton(withTitle: "Cancel Installation and Quit")
            alert.addButton(withTitle: "Keep Using BridgeVM").keyEquivalent = "\u{1b}"
            return alert.runModal() == .alertFirstButtonReturn
        },
        confirmAfterPreparation: {
            let alert = NSAlert()
            alert.messageText = "Finish preparing VM files and quit BridgeVM?"
            alert.informativeText = "You can follow progress or cancel the quit request while BridgeVM prepares the files."
            alert.addButton(withTitle: "Finish Then Quit")
            alert.addButton(withTitle: "Keep Using BridgeVM").keyEquivalent = "\u{1b}"
            return alert.runModal() == .alertFirstButtonReturn
        },
        revealLibrary: { ControlLibraryWindow.show() })

    static func shutdownDelayAlert(afterForce: Bool) -> NSAlert {
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = afterForce ? "The VM is still stopping." : "Windows is still shutting down."
        alert.informativeText = afterForce
            ? "Quitting now stops the VM immediately. Work that Windows has not saved may be lost."
            : "A Windows app may be waiting for you to save. Choose Cancel Quit to return to the VM, or keep waiting for Windows to shut down. Forcing a stop may lose unsaved work."
        alert.addButton(withTitle: "Keep Waiting")
        alert.addButton(withTitle: afterForce ? "Quit Anyway" : "Force Stop and Quit").hasDestructiveAction = true
        alert.addButton(withTitle: "Cancel Quit").keyEquivalent = "\u{1b}"
        return alert
    }
}
#endif
