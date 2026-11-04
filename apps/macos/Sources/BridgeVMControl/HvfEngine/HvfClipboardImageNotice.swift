import Combine
import SwiftUI

/// One plain notice when a copied image stayed where it was copied because it is over the
/// clipboard image limits. It follows the probe's exact skip and refusal lines
/// (agent_console/clip_image.rs); the next clipboard sync or a guest reset clears it.
@MainActor
final class HvfClipboardImageNotice: ObservableObject {
    enum Kind: Equatable { case macImageTooLarge, windowsImageTooLarge }

    static let macTooLargePrefix = "BVAGENT CLIPSYNC skip image too-large "
    static let windowsTooLargePrefix = "BVAGENT CLIPSYNC refused guest-image too-large "

    @Published private(set) var shown: Kind?

    func consume(_ event: BvAgentEvent) {
        switch event {
        case .clipSync:
            reset()
        case let .unknown(raw):
            let line = raw.hasSuffix("\r") ? String(raw.dropLast()) : raw
            if HvfGuestResetRecord.matches(line) {
                reset()
            } else if line.hasPrefix(Self.macTooLargePrefix) {
                show(.macImageTooLarge)
            } else if line.hasPrefix(Self.windowsTooLargePrefix) {
                show(.windowsImageTooLarge)
            }
        default:
            break
        }
    }

    func reset() {
        if shown != nil { shown = nil }
    }

    var displayed: (text: String, help: String)? {
        let help = "클립보드 이미지는 32MB, 16384×16384 픽셀까지 주고받습니다. 복사한 쪽의 클립보드는 그대로입니다."
        switch shown {
        case .macImageTooLarge: return ("Mac에서 복사한 이미지가 너무 커서 Windows로 보내지 않았습니다.", help)
        case .windowsImageTooLarge: return ("Windows에서 복사한 이미지가 너무 커서 Mac으로 가져오지 않았습니다.", help)
        case nil: return nil
        }
    }

    private func show(_ kind: Kind) {
        if shown != kind { shown = kind }
    }
}

struct HvfClipboardImageNoticeView: View {
    @ObservedObject var notice: HvfClipboardImageNotice

    var body: some View {
        if let shown = notice.displayed {
            VStack(alignment: .leading, spacing: 4) {
                Label(shown.text, systemImage: "photo")
                    .font(.callout).foregroundStyle(.orange)
                Text(shown.help)
                    .font(.caption).foregroundStyle(.secondary)
            }
            .textSelection(.enabled)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("bridgevm.runtime.clipboard.image.notice")
        }
    }
}
