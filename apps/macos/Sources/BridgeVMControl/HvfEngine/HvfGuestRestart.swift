import Foundation

/// What a Restart sent this boot. The guest reset record (or the end of the run) clears it.
enum HvfRestartRequest: Equatable, Sendable {
    /// The guest agent ran `shutdown.exe /r /t 0`; Windows asks each app to close first.
    case windowsAsked
    /// `RESET` went to the VMM: the boot ends at once, as after a power cut.
    case reset

    var message: String {
        switch self {
        case .windowsAsked:
            return "Windows에 다시 시작을 요청했습니다. 앱이 다시 시작을 막고 있으면 VM 화면을 확인하세요."
        case .reset:
            return "VM을 재설정하는 중입니다."
        }
    }
}

/// Why a Restart can only offer a Reset now.
enum HvfResetPrompt: Equatable, Sendable {
    /// No guest agent (login screen, early boot, an agent that stopped): Windows cannot be asked.
    case agentUnavailable
    /// Windows was asked and has not restarted (an app may be holding it); it can be asked again.
    case windowsHasNotRestarted
}

enum HvfRestartOutcome: Equatable, Sendable {
    case requested(HvfRestartRequest)
    /// Only a Reset is left; the caller asks the user before `requestReset()`.
    case needsResetConfirmation(HvfResetPrompt)
    case refused(String)

    /// What to show next to the control; nil while `restartRequest` already says it.
    var message: String? {
        if case let .refused(message) = self { return message }
        return nil
    }
}

extension HvfEngineSession {
    /// Windows asks each app to close, as the Start menu's restart does; no `/f`.
    static let guestRestartCommand = "shutdown.exe /r /t 0"
    /// The VMM ends the boot as a guest PSCI SYSTEM_RESET does (live_input.rs).
    static let resetLine = "RESET"

    /// Restart asks Windows through the guest agent, once per boot; otherwise only a Reset is left.
    func requestRestart() -> HvfRestartOutcome {
        guard vmmControlAvailable else { return .refused(Self.restartUnavailable) }
        if restartRequest == .reset { return .requested(.reset) }
        guard serviceStarted else { return .needsResetConfirmation(.agentUnavailable) }
        guard restartRequest == nil else { return .needsResetConfirmation(.windowsHasNotRestarted) }
        return requestWindowsRestart()
    }

    /// Asks Windows (again) to restart; with no agent to take it, only a Reset is left.
    func requestWindowsRestart() -> HvfRestartOutcome {
        guard vmmControlAvailable else { return .refused(Self.restartUnavailable) }
        guard serviceStarted, sendCtl(Self.guestRestartCommand) else {
            return .needsResetConfirmation(.agentUnavailable)
        }
        restartRequest = .windowsAsked
        append(.unknown("guest restart requested"))
        return .requested(.windowsAsked)
    }

    /// A hard reset: the VM restarts at once, and Windows loses what it has not saved.
    func requestReset() -> HvfRestartOutcome {
        guard vmmControlAvailable else { return .refused(Self.restartUnavailable) }
        // A second line could land in the next boot's helper and reset that boot too.
        if restartRequest == .reset { return .requested(.reset) }
        guard appendVMMControl(Self.resetLine) else {
            return .refused("재설정 요청을 보내지 못했습니다. 다시 시도하거나 VM을 중지하세요.")
        }
        restartRequest = .reset
        append(.unknown("VM reset requested"))
        return .requested(.reset)
    }

    private static let restartUnavailable = "실행 중인 VM이 없어 다시 시작할 수 없습니다."
}
