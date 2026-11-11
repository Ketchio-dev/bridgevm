import Combine
import SwiftUI

/// One plain notice when a copied image, or a file copy, stayed where it was copied because it
/// is over the clipboard limits or the receiving side lacks the room. It follows the probe's exact
/// skip and refusal lines
/// (agent_console/clip_image.rs, clip_files.rs) and is keyed to the copy they number (` copy=N`): that
/// copy's own text sync keeps it; a line about a later copy, a guest reset or a session
/// reset clears it.
@MainActor
final class HvfClipboardImageNotice: ObservableObject {
    enum Kind: Equatable {
        case macImageTooLarge, windowsImageTooLarge, macFilesTooLarge, windowsFilesTooLarge, macFilesUnreadable
        case windowsFilesLowSpace, macFilesGuestFull
    }

    static let macTooLargePrefix = "BVAGENT CLIPSYNC skip image too-large "
    static let windowsTooLargePrefix = "BVAGENT CLIPSYNC refused guest-image too-large "
    /// A Finder copy over the file limits goes to Windows as its names only.
    static let macFilesTooLargePrefix = "BVAGENT CLIPSYNC skip host-files too-large "
    static let windowsFilesTooLargePrefix = "BVAGENT CLIPSYNC refused guest-files too-large "
    /// A Finder copy with a file or folder that cannot be read also goes as its names only.
    static let macFilesUnreadablePrefix = "BVAGENT CLIPSYNC skip host-files unreadable "
    /// A large Windows copy needs its size plus the shared folder's 2 GB margin free on the Mac.
    static let windowsFilesLowSpacePrefix = "BVAGENT CLIPSYNC refused guest-files low-space "
    /// Windows refused a large Mac copy's file for want of disk space.
    static let macFilesGuestFullPrefix = "BVAGENT CLIPSYNC skip host-files guest-full "

    @Published private(set) var shown: Kind?
    /// The copy the notice is about; nil from a probe without copy numbers.
    private var copy: Int?
    /// The file limits a too-large line names (`limit-files=`, `limit-bytes=`): they depend on the
    /// guest agent (a current one takes 4096 entries and 16 GB). An older probe names none.
    private var fileLimits = (files: 256, bytes: Int64(64 << 20))

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
            } else if line.hasPrefix(Self.macFilesTooLargePrefix) {
                fileLimits = Self.fileLimits(in: line)
                show(.macFilesTooLarge, copy: Self.copy(in: line))
            } else if line.hasPrefix(Self.windowsFilesTooLargePrefix) {
                fileLimits = Self.fileLimits(in: line)
                show(.windowsFilesTooLarge, copy: Self.copy(in: line))
            } else if line.hasPrefix(Self.macFilesUnreadablePrefix) {
                show(.macFilesUnreadable, copy: Self.copy(in: line))
            } else if line.hasPrefix(Self.windowsFilesLowSpacePrefix) {
                show(.windowsFilesLowSpace, copy: Self.copy(in: line))
            } else if line.hasPrefix(Self.macFilesGuestFullPrefix) {
                show(.macFilesGuestFull, copy: Self.copy(in: line))
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

    var displayed: (text: String, help: String, symbol: String)? {
        let help = "클립보드 이미지는 32MB, 16384×16384 픽셀까지 주고받습니다. 복사한 쪽의 클립보드는 그대로입니다."
        let limits = "한 번에 \(fileLimits.files)개, 모두 합쳐 \(Self.size(fileLimits.bytes))까지"
        // The shared folder carries files up to 16 GB each (HvfEngineConfig.shareMaxKiB), any number of them.
        let windowsFilesHelp = "파일 복사는 \(limits) 가져옵니다. 더 큰 파일은 공유 폴더를 켜고 그 안에 넣으면 Mac으로 옮겨집니다."
        // Finder drops have no size limit (file_drop.rs streams them to the guest Desktop).
        let macFilesHelp = "파일 복사는 \(limits) 보냅니다. 더 큰 파일은 VM 창으로 끌어다 놓으면 Windows 바탕 화면에 복사됩니다."
        switch shown {
        case .macImageTooLarge: return ("Mac에서 복사한 이미지가 너무 커서 Windows로 보내지 않았습니다.", help, "photo")
        case .windowsImageTooLarge: return ("Windows에서 복사한 이미지가 너무 커서 Mac으로 가져오지 않았습니다.", help, "photo")
        case .macFilesTooLarge:
            return ("Mac에서 복사한 파일이 너무 많거나 커서 Windows에는 파일 이름만 보냈습니다.", macFilesHelp, "doc.on.doc")
        case .windowsFilesTooLarge:
            return ("Windows에서 복사한 파일이 너무 많거나 커서 Mac으로 가져오지 않았습니다.", windowsFilesHelp, "doc.on.doc")
        case .macFilesUnreadable:
            return ("Mac에서 복사한 파일 중 읽을 수 없는 것이 있어 Windows에는 파일 이름만 보냈습니다.",
                    "읽기 권한이 없는 파일이나 폴더가 있는지 확인한 뒤 다시 복사하세요.", "doc.on.doc")
        case .windowsFilesLowSpace:
            return ("Mac의 저장 공간이 부족해 Windows에서 복사한 파일을 가져오지 않았습니다.",
                    "복사한 파일 크기에 2GB를 더한 만큼 Mac에 빈 공간이 있어야 합니다. 공간을 비운 뒤 다시 복사하세요.", "doc.on.doc")
        case .macFilesGuestFull:
            return ("Windows 디스크 공간이 부족해 Mac에서 복사한 파일을 보내지 않았습니다.",
                    "Windows에서 공간을 비운 뒤 다시 복사하세요.", "doc.on.doc")
        case nil: return nil
        }
    }

    private func show(_ kind: Kind, copy: Int?) {
        self.copy = copy
        if shown != kind { shown = kind }
    }

    private static func copy(in line: String) -> Int? {
        number(" copy=", in: line).flatMap { Int(exactly: $0) }
    }

    private static func number(_ key: String, in line: String) -> Int64? {
        guard let range = line.range(of: key) else { return nil }
        return Int64(line[range.upperBound...].prefix { $0.isNumber })
    }

    /// A too-large line's limits; an older probe's line names none: 256 entries and 64 MB.
    private static func fileLimits(in line: String) -> (files: Int, bytes: Int64) {
        let files = number(" limit-files=", in: line).flatMap { Int(exactly: $0) } ?? 256
        return (files, number(" limit-bytes=", in: line) ?? Int64(64 << 20))
    }

    /// 64MB, 16GB: whole GB where they are whole, else MB.
    private static func size(_ bytes: Int64) -> String {
        bytes >= 1 << 30 && bytes % (1 << 30) == 0 ? "\(bytes >> 30)GB" : "\(bytes >> 20)MB"
    }
}

struct HvfClipboardImageNoticeView: View {
    @ObservedObject var notice: HvfClipboardImageNotice

    var body: some View {
        if let shown = notice.displayed {
            VStack(alignment: .leading, spacing: 4) {
                Label(shown.text, systemImage: shown.symbol)
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
