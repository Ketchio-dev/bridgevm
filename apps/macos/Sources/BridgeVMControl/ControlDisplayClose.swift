#if canImport(AppKit)
import AppKit

enum ControlDisplayCloseChoice: String, CaseIterable {
    case background, pause, shutdown, cancel
    var title: String {
        switch self {
        case .background: return "Keep running in the background"
        case .pause: return "Pause Windows"
        case .shutdown: return "Shut down Windows"
        case .cancel: return "Cancel"
        }
    }
    /// Shut down keeps the window open: an app with unsaved work holds the shutdown on Windows' own
    /// screen, and the window then says that Windows has shut down.
    var closesWindow: Bool { self != .shutdown }
}

enum ControlDisplayClosePlan: Equatable {
    case close, ask([ControlDisplayCloseChoice]), apply(ControlDisplayCloseChoice)
    /// A paused VM stays paused and one already stopping goes on stopping, so their window closes
    /// without a question. Shut down is offered only when the Stop button would work.
    @MainActor
    static func make(session: HvfEngineSession, quitting: Bool, remembered: ControlDisplayCloseChoice?) -> Self {
        guard session.hasActiveRuntimeWork, !quitting, !session.pause.showsResume,
              session.connectionState != .stopping else { return .close }
        let canPause = session.pauseAvailable && session.pause == .running
        let canStop = session.ownedProcessIdentity != nil && !session.runtimeStartupWorkerPending
        let choices = ControlDisplayCloseChoice.allCases.filter { ($0 != .pause || canPause) && ($0 != .shutdown || canStop) }
        if let remembered, remembered != .cancel, choices.contains(remembered) { return .apply(remembered) }
        return .ask(choices)
    }
}

@MainActor
enum ControlDisplayClose {
    static let preferenceKey = "runningDisplayCloseChoice"
    static var quitting = false
    static func alert(choices: [ControlDisplayCloseChoice]) -> NSAlert {
        let alert = NSAlert()
        alert.messageText = "What should Windows do when you close this window?"
        choices.forEach {
            let button = alert.addButton(withTitle: $0.title)
            if $0 == .cancel { button.keyEquivalent = "\u{1b}" }
        }
        alert.showsSuppressionButton = true
        alert.suppressionButton?.title = "Remember my choice"
        return alert
    }
    static func remembered(defaults: UserDefaults = .standard) -> ControlDisplayCloseChoice? {
        defaults.string(forKey: preferenceKey).flatMap(ControlDisplayCloseChoice.init(rawValue:))
    }
    static func remember(_ choice: ControlDisplayCloseChoice?, defaults: UserDefaults = .standard) {
        defaults.set(choice == .cancel ? nil : choice?.rawValue, forKey: preferenceKey)
    }
    /// Whether the choice took effect. Cancel never does. Only refusals are reported, a stop refusal with
    /// the ordinary Stop action's message: after Shut down the window shows Windows shutting down.
    static func apply(_ choice: ControlDisplayCloseChoice, session: HvfEngineSession, report: (String) -> Void) -> Bool {
        switch choice {
        case .cancel: return false
        case .background: return true
        case .pause:
            if let message = session.requestPause().message { report(message); return false }
            return true
        case .shutdown:
            var message = ""
            let accepted = HvfRuntimeStopAction.perform(session: session,
                requestStop: { $0.stopOwned(expectedToken: $1.token) }, report: { message = $0 }, refresh: { session.poll() })
            if !accepted { report(message) }
            return accepted
        }
    }
}
#endif
