import Foundation

/// What a Restart sent.
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

/// A request and the guest boot it targets. The boot's reset record completes it; any boot change
/// clears it (HvfEngineSession.guestBootChanged), so Restart is always available again.
struct HvfPendingRestart: Equatable, Sendable {
    let kind: HvfRestartRequest
    let boot: Int
    let sentAt: TimeInterval
    /// The VMM took the `RESET` line (`BVPOWER host reset`); the reset record follows.
    var acknowledged = false
}

/// Why a Restart can only offer a Reset now.
enum HvfResetPrompt: Equatable, Sendable {
    /// No guest agent (login screen, early boot, an agent that stopped): Windows cannot be asked.
    case agentUnavailable
    /// Windows was asked and has not restarted (an app may be holding it); it can be asked again.
    case windowsHasNotRestarted
}

/// The open Reset confirmation and the guest boot it was asked for.
struct HvfResetConfirmation: Equatable, Sendable {
    let prompt: HvfResetPrompt
    let boot: Int
}

enum HvfRestartOutcome: Equatable, Sendable {
    case requested(HvfRestartRequest)
    /// Only a Reset is left; the user answers `resetConfirmation` with `requestReset()`.
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
    /// The VMM's reply to a `RESET` line it took; the reset record follows.
    static let resetAcknowledgement = "BVPOWER host reset"
    /// A running helper takes `RESET` within milliseconds. A line written after a reset record but
    /// before the next helper opens input.ctl is never read (it starts at the end), so an
    /// unacknowledged Reset is dropped after this long and Restart works again.
    static let resetAcknowledgementTimeout: TimeInterval = 5

    /// Restart asks Windows through the guest agent, once per boot; otherwise only a Reset is left,
    /// after the user confirms it (`resetConfirmation`, answered by `requestReset()`).
    func requestRestart() -> HvfRestartOutcome {
        guard vmmControlAvailable else { return .refused(Self.restartUnavailable) }
        if let pending = restartRequest, pending.kind == .reset { return .requested(.reset) }
        let prompt: HvfResetPrompt
        if !serviceStarted {
            prompt = .agentUnavailable
        } else if restartRequest != nil {
            prompt = .windowsHasNotRestarted
        } else if askWindows() {
            return .requested(.windowsAsked)
        } else {
            prompt = .agentUnavailable
        }
        resetConfirmation = HvfResetConfirmation(prompt: prompt, boot: guestBoot)
        return .needsResetConfirmation(prompt)
    }

    /// The user asks Windows again from the confirmation.
    func requestWindowsRestart() -> HvfRestartOutcome {
        guard takeConfirmation() != nil else { return .refused(Self.bootChanged) }
        guard vmmControlAvailable else { return .refused(Self.restartUnavailable) }
        return askWindows() ? .requested(.windowsAsked) : .refused(Self.agentUnavailable)
    }

    /// The user confirmed the Reset offered for this boot: the VM restarts at once, and Windows loses
    /// what it has not saved.
    func requestReset() -> HvfRestartOutcome {
        guard takeConfirmation() != nil else { return .refused(Self.bootChanged) }
        guard vmmControlAvailable else { return .refused(Self.restartUnavailable) }
        if let pending = restartRequest, pending.kind == .reset { return .requested(.reset) }
        guard appendVMMControl(Self.resetLine) else {
            return .refused("재설정 요청을 보내지 못했습니다. 다시 시도하거나 VM을 중지하세요.")
        }
        restartRequest = HvfPendingRestart(kind: .reset, boot: guestBoot, sentAt: ownedStartNow())
        append(.unknown("VM reset requested"))
        return .requested(.reset)
    }

    func cancelResetConfirmation() { resetConfirmation = nil }

    /// A guest reset record, or the run starting or ending: the next boot is a different one.
    func guestBootChanged() {
        guestBoot += 1
        pauseEnded()
        agentSilenceNotice.reset()
        guestToolsMessage = nil
        if restartRequest != nil { restartRequest = nil }
        if resetConfirmation != nil { resetConfirmation = nil }
        if restartNotice != nil { restartNotice = nil }
    }

    /// run.log said the VMM took a `RESET`.
    func resetAcknowledged() {
        guard var pending = restartRequest, pending.kind == .reset, pending.boot == guestBoot else { return }
        pending.acknowledged = true
        restartRequest = pending
    }

    /// From poll(): a Reset no helper took is dropped, so the user can press again.
    func expireUnacknowledgedReset() {
        guard let pending = restartRequest, pending.kind == .reset, !pending.acknowledged,
              ownedStartNow() - pending.sentAt >= Self.resetAcknowledgementTimeout else { return }
        restartRequest = nil
        restartNotice = "VM이 재설정 요청을 받지 못했습니다. 다시 시작을 한 번 더 누르세요."
        append(.unknown("VM reset request not taken; Restart is available again"))
    }

    private func askWindows() -> Bool {
        guard serviceStarted else { return false }
        resumeBeforeStop() // a paused Windows cannot restart
        guard sendCtl(Self.guestRestartCommand) else { return false }
        restartRequest = HvfPendingRestart(kind: .windowsAsked, boot: guestBoot, sentAt: ownedStartNow())
        restartNotice = nil
        append(.unknown("guest restart requested"))
        return true
    }

    /// The open confirmation, if it was asked for this boot; it is used up either way.
    private func takeConfirmation() -> HvfResetConfirmation? {
        defer { resetConfirmation = nil }
        guard let confirmation = resetConfirmation, confirmation.boot == guestBoot else { return nil }
        return confirmation
    }

    private static let restartUnavailable = "실행 중인 VM이 없어 다시 시작할 수 없습니다."
    private static let agentUnavailable = "Windows 에이전트가 응답하지 않아 다시 시작을 요청하지 못했습니다."
    private static let bootChanged = "그사이 Windows가 다시 시작되어 요청을 보내지 않았습니다."
}
