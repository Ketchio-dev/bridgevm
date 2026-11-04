#if canImport(AppKit)
import AppKit

/// One VM run whose runner the app's exit would cancel (its owner input closes).
@MainActor
protocol ControlAppTerminationSession: AnyObject {
    /// True until the run's teardown is confirmed, or while a start may still spawn a runner.
    var blocksAppTermination: Bool { get }
    /// The user's Stop; repeating it joins the stop already admitted.
    func requestTerminationStop()
    /// Force Stop for a stop that is still waiting on Windows.
    func requestTerminationForceStop()
    /// Reads the run's progress where its own default-mode timer cannot fire.
    func pollTerminationWait()
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
    private var pending = false
    private var forced = false
    private var deciding = false
    private var deadlineAt: TimeInterval = 0
    private var timer: Timer?

    init(sessions: @escaping @MainActor () -> [ControlAppTerminationSession], presenter: Presenter,
         reply: @escaping @MainActor (Bool) -> Void,
         now: @escaping () -> TimeInterval = { ProcessInfo.processInfo.systemUptime }) {
        self.sessions = sessions; self.presenter = presenter; self.reply = reply; self.now = now
    }

    func shouldTerminate(reason: QuitReason) -> NSApplication.TerminateReply {
        guard !pending else { return .terminateCancel } // the pending quit answers for itself
        let running = blocking()
        switch Self.plan(blockingSessions: running.count, reason: reason) {
        case .terminateNow: return .terminateNow
        case .confirmThenStop: guard presenter.confirmQuit(running.count) else { return .terminateCancel }
        case .stopWithoutConfirmation: break
        }
        pending = true; forced = false
        deadlineAt = now() + Self.deadline
        running.forEach { $0.requestTerminationStop() }
        startPolling()
        return .terminateLater
    }

    /// One wait step: re-requests stops a pending start could not take yet, answers AppKit once
    /// every run is down, and asks the user at the deadline.
    func poll() {
        guard pending, !deciding else { return }
        let running = blocking()
        guard !running.isEmpty else { finish(true); return }
        running.forEach { $0.requestTerminationStop() }
        guard now() >= deadlineAt else { return }
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

extension ControlAppTermination.Presenter {
    static let alerts = Self(
        confirmQuit: { count in
            let alert = NSAlert()
            alert.messageText = "Windows를 안전하게 종료한 뒤 BridgeVM을 끝낼까요?"
            alert.informativeText = "실행 중인 VM \(count)개의 Windows에 종료를 요청하고, 모두 꺼지면 BridgeVM을 끝냅니다. "
                + "저장하지 않은 앱이 있으면 Windows가 저장할 기회를 줍니다."
            alert.addButton(withTitle: "Windows 종료 후 끝내기")
            alert.addButton(withTitle: "취소")
            return alert.runModal() == .alertFirstButtonReturn
        },
        decideAtDeadline: { afterForce, stillRunning in
            let alert = NSAlert()
            alert.alertStyle = .warning
            alert.messageText = "Windows가 아직 종료되지 않았습니다."
            alert.informativeText = afterForce
                ? "강제 중단한 VM이 아직 멈추지 않았습니다. 그래도 끝내면 BridgeVM이 VM을 바로 멈춥니다(디스크와 UEFI 설정은 저장합니다)."
                : "저장하지 않은 앱이 Windows 종료를 막고 있을 수 있습니다. 확인하려면 종료를 취소하고 VM 화면을 보세요. "
                    + "강제 중단하면 Windows가 앱을 저장하지 않고 닫습니다."
            alert.addButton(withTitle: afterForce ? "그래도 끝내기" : "강제 중단 후 끝내기")
            alert.addButton(withTitle: "계속 기다리기")
            alert.addButton(withTitle: "종료 취소")
            // Every run may still stop while the question is open: then the quit goes on by itself.
            let watcher = Timer(timeInterval: 0.25, repeats: true) { _ in
                MainActor.assumeIsolated { if !stillRunning() { NSApp.abortModal() } }
            }
            RunLoop.main.add(watcher, forMode: .modalPanel)
            defer { watcher.invalidate() }
            switch alert.runModal() {
            case .alertFirstButtonReturn: return afterForce ? .quitAnyway : .forceStop
            case .alertThirdButtonReturn: return .cancelQuit
            default: return .keepWaiting
            }
        })
}
#endif
