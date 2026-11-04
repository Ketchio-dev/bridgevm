import Combine
import SwiftUI

/// Guest tools updates. The probe installs the app's copy of the guest agent over an older one
/// (agent_console/agent_update.rs), at most once per app session per VM; the VM bundle's
/// `metadata/agent-update.json` records the last attempt.
enum HvfAgentUpdate {
    /// One ID per app launch: the probe tries a VM's update once per ID.
    static let session = UUID().uuidString.lowercased()

    /// The runner's update options for a library VM; `assetsDir` holds the app's agent files.
    static func runnerArguments(bundlePath: String, assetsDir: String) -> [String] {
        ["--helper-agent-update", assetsDir,
         "--helper-agent-update-marker", bundlePath + "/metadata/agent-update.json",
         "--helper-agent-update-session", session]
    }

    /// The one line the user sees for a probe result, or nil for other lines.
    static func notice(for line: String) -> String? {
        let prefix = "BVAGENT UPDATE "
        guard line.hasPrefix(prefix) else { return nil }
        switch line.dropFirst(prefix.count).prefix(while: { $0 != " " }) {
        case "ok": return "게스트 도구를 새 버전으로 업데이트했습니다."
        case "failed": return "게스트 도구 업데이트에 실패해 이전 버전을 계속 사용합니다. 앱을 다시 시작하면 다시 시도합니다."
        case "rolled-back": return "새 게스트 도구가 시작되지 않아 이전 버전으로 되돌렸습니다. 앱을 다시 시작하면 다시 시도합니다."
        default: return nil
        }
    }
}

/// The latest update result of this run; cleared when the session resets.
@MainActor
final class HvfAgentUpdateNotice: ObservableObject {
    @Published private(set) var text: String?

    func consume(_ event: BvAgentEvent) {
        guard case let .unknown(raw) = event else { return }
        let line = raw.hasSuffix("\r") ? String(raw.dropLast()) : raw
        if let notice = HvfAgentUpdate.notice(for: line), notice != text { text = notice }
    }

    func reset() {
        if text != nil { text = nil }
    }
}

struct HvfAgentUpdateNoticeView: View {
    @ObservedObject var notice: HvfAgentUpdateNotice

    var body: some View {
        if let text = notice.text {
            Label(text, systemImage: "arrow.triangle.2.circlepath")
                .font(.callout).foregroundStyle(.secondary)
                .textSelection(.enabled)
                .accessibilityIdentifier("bridgevm.runtime.agent-update.notice")
        }
    }
}
