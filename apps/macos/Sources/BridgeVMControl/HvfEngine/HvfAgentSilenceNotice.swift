import Combine
import SwiftUI

/// One plain notice while the guest agent, which carries the clipboard, the shared folder, the
/// display's window-size switch and the clock resync, is not there: no `SERVICE start` a while after
/// Windows started, or the probe reporting a keepalive request it is still waiting on (`BVAGENT
/// SERVICE overdue`, agent_console/resident_service.rs). It clears by itself once the agent answers again,
/// and it is off while the VM is paused, stopping or restarting (HvfEngineSessionAgentSilence).
@MainActor
final class HvfAgentSilenceNotice: ObservableObject {
    enum Kind: Equatable { case notConnected, notResponding, missingTools, awaitingFirstConnection }

    /// The scoreboards measured READY 7-26 s after launch (vm_bench medians, T16-T42) on images that
    /// log on by themselves. A product VM starts its agent at the user's logon, and the agent's task
    /// starts it again within a minute when that start fails (bvagent-task.ps1): 30 s, plus that
    /// minute, plus 30 s.
    static let readyTimeout: TimeInterval = 120
    /// The probe repeats the overdue line every 20 s (SERVICE_OVERDUE_INTERVAL) while a request
    /// waits, and prints `SERVICE alive` every 30 s: an overdue line 30 s of probe time behind the
    /// newest line is over. Probe time stands still while the VM is paused and is rebuilt for each boot.
    static let overdueWindowMs = 30_000
    /// The probe reports any request overdue after 20 s, but most may take longer while the agent is
    /// well: the app's own guest commands (`ctl`: a display-driver install, a disk extend), a clipboard
    /// image or file read, an agent update, a Finder drop, a big shared folder, a clock resync. Only the
    /// quick keepalive requests answer within 20 s from a healthy agent (req_kind, agent_console/share.rs).
    static let quickKinds: Set<String> = ["ping", "clip-poll", "clip-caps"]

    static let notConnectedText = "Clipboard sharing, shared folders, and automatic resizing are waiting for Windows to connect. If they stay unavailable after you sign in, restart Windows."
    static let notRespondingText = "Guest Tools stopped responding. If clipboard sharing, shared folders, or automatic resizing stay unavailable, restart Windows."

    @Published private(set) var shown: Kind?
    /// The user closed the strip over the display; it comes back with the next notice.
    @Published private(set) var dismissedInWindow = false

    private var waitingSince: TimeInterval?
    private var agentSeen: Bool?
    private var overdueMs: Int?
    private var latestMs = 0

    /// A keepalive request is overdue, also while the notice is off: a Stop that asked the agent to shut
    /// Windows down presses the power button too (HvfOwnedRunController.guestAgentSilence).
    var agentOverdue: Bool {
        guard let overdueMs else { return false }
        return latestMs - overdueMs < Self.overdueWindowMs
    }

    var text: String? {
        switch shown {
        case .awaitingFirstConnection: return "Waiting for Windows to connect. Complete any setup or sign-in steps in the VM window."
        case .missingTools: return HvfGuestTools.missing
        case .notConnected: return Self.notConnectedText
        case .notResponding: return Self.notRespondingText
        case nil: return nil
        }
    }

    /// Missing tools have their own install strip; the first connection uses the window subtitle.
    var windowConnectionWarning: String? {
        guard !dismissedInWindow, shown == .notConnected || shown == .notResponding else { return nil }
        return text
    }

    func consume(_ event: BvAgentEvent) {
        switch event {
        case let .overdue(kind, _, tMs) where Self.quickKinds.contains(kind):
            overdueMs = tMs
            note(tMs)
        case .overdue:
            break // a slow request: neither a sign of silence nor, on its own, of an answer
        case let .ready(_, tMs), let .serviceStart(tMs):
            overdueMs = nil // the agent spoke
            note(tMs)
        case let .unknown(line) where line.hasPrefix("BVAGENT re-READY "):
            overdueMs = nil
        case let .aliveHeartbeat(tMs), let .clipSync(_, _, tMs, _), let .shareEvent(_, _, _, tMs):
            note(tMs)
        default:
            break
        }
    }

    /// From every runtime poll. `running`: the VM runs, not paused, stopping or restarting.
    /// `agentInstalled` is asked once per boot, when the wait is over.
    func refresh(now: TimeInterval, running: Bool, serviceStarted: Bool,
                 firstConnection: () -> Kind?, agentInstalled: () -> Bool) {
        var kind: Kind?
        if !running {
            waitingSince = nil
        } else if serviceStarted {
            waitingSince = nil
            if agentOverdue { kind = .notResponding }
        } else {
            let since = waitingSince ?? now
            waitingSince = since
            if now - since >= Self.readyTimeout {
                if agentSeen == nil { agentSeen = agentInstalled() }
                if agentSeen == true { kind = .notConnected }
                else { kind = firstConnection() }
            }
        }
        show(kind)
    }

    /// A guest reset, or a run starting or ending: the next boot starts over.
    func reset() {
        waitingSince = nil
        agentSeen = nil
        overdueMs = nil
        latestMs = 0
        show(nil)
    }

    func dismissInWindow() {
        if shown != nil, !dismissedInWindow { dismissedInWindow = true }
    }

    private func note(_ tMs: Int) { latestMs = max(latestMs, tMs) }

    private func show(_ kind: Kind?) {
        guard kind != shown else { return }
        shown = kind
        if dismissedInWindow { dismissedInWindow = false }
    }
}

/// The notice in the dashboard's and the runtime card's notice lists.
struct HvfAgentSilenceNoticeView: View {
    @ObservedObject var notice: HvfAgentSilenceNotice

    var body: some View {
        if let text = notice.text {
            let waiting = notice.shown == .awaitingFirstConnection
            Label(text, systemImage: waiting ? "info.circle" : "exclamationmark.triangle")
                .font(.callout).foregroundStyle(waiting ? Color.secondary : .orange)
                .textSelection(.enabled)
                .accessibilityIdentifier("bridgevm.runtime.agent-silence.notice")
        }
    }
}

/// The notice over the VM display: a small strip at the top centre, clear of the taskbar and of a
/// maximized app's caption buttons, which the user can close.
struct HvfAgentSilenceStrip: View {
    @ObservedObject var notice: HvfAgentSilenceNotice

    var body: some View {
        if let text = notice.windowConnectionWarning {
            HvfNoticeStrip(identifier: "bridgevm.display.agent-silence", dismiss: { notice.dismissInWindow() }) {
                Label {
                    Text(text).fixedSize(horizontal: false, vertical: true)
                } icon: {
                    Image(systemName: "exclamationmark.triangle").foregroundStyle(.orange)
                }
            }
        }
    }
}

/// Shared presentation for actionable, dismissible notices over the VM display.
struct HvfNoticeStrip<Content: View>: View {
    let identifier: String
    let dismiss: () -> Void
    @ViewBuilder var content: () -> Content

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            content()
            Button(action: dismiss) { Image(systemName: "xmark") }
                .buttonStyle(.plain)
                .help("Dismiss notice")
                .accessibilityLabel("Dismiss notice")
                .accessibilityIdentifier(identifier + ".close")
        }
            .font(.callout)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 10))
            .frame(maxWidth: 640)
            .padding(8)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(identifier)
    }
}
