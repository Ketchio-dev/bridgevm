#if canImport(AppKit)
import AppKit

enum ControlDisplayCloseChoice: String, CaseIterable {
    case background, pause, shutdown, cancel
    var title: String {
        switch self {
        case .background: return "백그라운드에서 계속 실행"
        case .pause: return "일시 정지"
        case .shutdown: return "Windows 종료"
        case .cancel: return "취소"
        }
    }
}

enum ControlDisplayClosePlan: Equatable {
    case close, ask([ControlDisplayCloseChoice]), apply(ControlDisplayCloseChoice)
    static func make(running: Bool, quitting: Bool, canPause: Bool, remembered: ControlDisplayCloseChoice?) -> Self {
        guard running, !quitting else { return .close }
        let choices: [ControlDisplayCloseChoice] = canPause ? [.background, .pause, .shutdown, .cancel] : [.background, .shutdown, .cancel]
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
        alert.messageText = "창을 닫은 뒤 Windows를 어떻게 할까요?"
        choices.forEach {
            let button = alert.addButton(withTitle: $0.title)
            if $0 == .cancel { button.keyEquivalent = "\u{1b}" }
        }
        alert.showsSuppressionButton = true
        alert.suppressionButton?.title = "다시 묻지 않음"
        return alert
    }
    static func remembered(defaults: UserDefaults = .standard) -> ControlDisplayCloseChoice? {
        defaults.string(forKey: preferenceKey).flatMap(ControlDisplayCloseChoice.init(rawValue:))
    }
    static func remember(_ choice: ControlDisplayCloseChoice?, defaults: UserDefaults = .standard) {
        defaults.set(choice == .cancel ? nil : choice?.rawValue, forKey: preferenceKey)
    }
    /// Cancel never closes or persists; stop refusals use the ordinary Stop action's message.
    static func apply(_ choice: ControlDisplayCloseChoice, session: HvfEngineSession, report: (String) -> Void) -> Bool {
        switch choice {
        case .cancel: return false
        case .background: return true
        case .pause:
            if let message = session.requestPause().message { report(message); return false }
            return true
        case .shutdown:
            return HvfRuntimeStopAction.perform(session: session,
                requestStop: { $0.stopOwned(expectedToken: $1.token) }, report: report, refresh: { session.poll() })
        }
    }
}
#endif
