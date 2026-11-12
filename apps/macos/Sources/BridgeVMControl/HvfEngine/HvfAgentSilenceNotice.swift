import Combine
import SwiftUI

/// One plain notice while the guest agent, which carries the clipboard, the shared folder, the
/// display's window-size switch and the clock resync, is not there: no `SERVICE start` a while after
/// Windows started, or the probe reporting a keepalive request it is still waiting on (`BVAGENT
/// SERVICE overdue`, agent_console/resident_service.rs). It clears by itself once the agent answers again,
/// and it is off while the VM is paused, stopping or restarting (HvfEngineSessionAgentSilence).
@MainActor
final class HvfAgentSilenceNotice: ObservableObject {
    enum Kind: Equatable { case notConnected, notResponding }

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

    static let notConnectedText = "Windows 에이전트가 아직 연결되지 않아 클립보드·공유 폴더·화면 크기 맞춤이 동작하지 않습니다. Windows에 로그인한 뒤에도 계속되면 Windows를 다시 시작하세요."
    static let notRespondingText = "Windows 에이전트가 응답하지 않아 클립보드·공유 폴더·화면 크기 맞춤이 잠시 동작하지 않습니다. 계속되면 Windows를 다시 시작하세요."

    @Published private(set) var shown: Kind?
    /// The user closed the strip over the display; it comes back with the next notice.
    @Published private(set) var dismissedInWindow = false

    private var waitingSince: TimeInterval?
    private var agentSeen: Bool?
    private var overdueMs: Int?
    private var latestMs = 0

    var text: String? {
        switch shown {
        case .notConnected: return Self.notConnectedText
        case .notResponding: return Self.notRespondingText
        case nil: return nil
        }
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
    func refresh(now: TimeInterval, running: Bool, serviceStarted: Bool, agentInstalled: () -> Bool) {
        var kind: Kind?
        if !running {
            waitingSince = nil
        } else if serviceStarted {
            waitingSince = nil
            if let overdueMs, latestMs - overdueMs < Self.overdueWindowMs { kind = .notResponding }
        } else {
            let since = waitingSince ?? now
            waitingSince = since
            if now - since >= Self.readyTimeout {
                if agentSeen == nil { agentSeen = agentInstalled() }
                if agentSeen == true { kind = .notConnected }
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
            Label(text, systemImage: "exclamationmark.triangle")
                .font(.callout).foregroundStyle(.orange)
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
        if let text = notice.text, !notice.dismissedInWindow {
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
                .help("닫기")
                .accessibilityLabel("닫기")
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
