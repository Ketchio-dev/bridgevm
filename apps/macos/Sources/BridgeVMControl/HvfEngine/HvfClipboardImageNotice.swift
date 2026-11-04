import Combine
import SwiftUI

/// One plain notice when a copied image stayed where it was copied because it is over the
/// clipboard image limits. It follows the probe's exact skip and refusal lines
/// (agent_console/clip_image.rs) and is keyed to the copy they number (` copy=N`): that
/// copy's own text sync keeps it; a line about a later copy, a guest reset or a session
/// reset clears it.
@MainActor
final class HvfClipboardImageNotice: ObservableObject {
    enum Kind: Equatable { case macImageTooLarge, windowsImageTooLarge }

    static let macTooLargePrefix = "BVAGENT CLIPSYNC skip image too-large "
    static let windowsTooLargePrefix = "BVAGENT CLIPSYNC refused guest-image too-large "

    @Published private(set) var shown: Kind?
    /// The copy the notice is about; nil from a probe without copy numbers.
    private var copy: Int?

    func consume(_ event: BvAgentEvent) {
        switch event {
        case let .clipSync(_, _, _, copy):
            clear(unlessAbout: copy)
        case let .unknown(raw):
            let line = raw.hasSuffix("\r") ? String(raw.dropLast()) : raw
            if HvfGuestResetRecord.matches(line) {
                reset()
            } else if line.hasPrefix(Self.macTooLargePrefix) {
                show(.macImageTooLarge, copy: Self.copy(in: line))
            } else if line.hasPrefix(Self.windowsTooLargePrefix) {
                show(.windowsImageTooLarge, copy: Self.copy(in: line))
            } else if line.hasPrefix("BVAGENT CLIPSYNC "), let lineCopy = Self.copy(in: line) {
                clear(unlessAbout: lineCopy) // a later copy refused for another reason
            }
        default:
            break
        }
    }

    func reset() {
        copy = nil
        if shown != nil { shown = nil }
    }

    /// A line about the notice's copy (or an earlier one) keeps it; a later copy's clears it.
    /// Without copy numbers every sync counts as later, as before.
    private func clear(unlessAbout lineCopy: Int?) {
        if let lineCopy, let copy, lineCopy <= copy { return }
        reset()
    }

    var displayed: (text: String, help: String)? {
        let help = "클립보드 이미지는 32MB, 16384×16384 픽셀까지 주고받습니다. 복사한 쪽의 클립보드는 그대로입니다."
        switch shown {
        case .macImageTooLarge: return ("Mac에서 복사한 이미지가 너무 커서 Windows로 보내지 않았습니다.", help)
        case .windowsImageTooLarge: return ("Windows에서 복사한 이미지가 너무 커서 Mac으로 가져오지 않았습니다.", help)
        case nil: return nil
        }
    }

    private func show(_ kind: Kind, copy: Int?) {
        self.copy = copy
        if shown != kind { shown = kind }
    }

    private static func copy(in line: String) -> Int? {
        guard let range = line.range(of: " copy=") else { return nil }
        return Int(line[range.upperBound...].prefix { $0.isNumber })
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
