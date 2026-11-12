#if canImport(AppKit)
import AppKit

enum ControlDisplayCloseChoice: String, CaseIterable {
    case background, pause, shutdown
    var title: String {
        switch self {
        case .background: return "Windows를 백그라운드에서 계속 실행"
        case .pause: return "Windows 일시 정지"
        case .shutdown: return "Windows 종료"
        }
    }
}

enum ControlDisplayClosePlan: Equatable {
    case close, ask([ControlDisplayCloseChoice]), apply(ControlDisplayCloseChoice)
    static func make(running: Bool, quitting: Bool, canPause: Bool, remembered: ControlDisplayCloseChoice?) -> Self {
        guard running, !quitting else { return .close }
        let choices: [ControlDisplayCloseChoice] = canPause ? [.background, .pause, .shutdown] : [.background, .shutdown]
        if let remembered, choices.contains(remembered) { return .apply(remembered) }
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
        choices.forEach { alert.addButton(withTitle: $0.title) }
        alert.showsSuppressionButton = true
        alert.suppressionButton?.title = "다시 묻지 않음"
        return alert
    }
    static func decide(running: Bool, canPause: Bool, defaults: UserDefaults = .standard) -> ControlDisplayCloseChoice? {
        let plan = ControlDisplayClosePlan.make(running: running, quitting: quitting, canPause: canPause,
            remembered: defaults.string(forKey: preferenceKey).flatMap(ControlDisplayCloseChoice.init(rawValue:)))
        switch plan {
        case .close: return nil
        case let .apply(choice): return choice
        case let .ask(choices):
            let alert = alert(choices: choices)
            let response = alert.runModal().rawValue - NSApplication.ModalResponse.alertFirstButtonReturn.rawValue
            guard choices.indices.contains(response) else { return nil }
            let choice = choices[response]
            if alert.suppressionButton?.state == .on { defaults.set(choice.rawValue, forKey: preferenceKey) }
            return choice
        }
    }
}
#endif
