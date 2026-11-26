import Combine
import SwiftUI

/// Shows why a clipboard transfer stopped. Numbered relay records keep the notice
/// with its copy until a later copy or a guest/session reset clears it.
@MainActor
final class HvfClipboardImageNotice: ObservableObject {
    enum Kind: Equatable {
        case macTextTooLarge, windowsTextTooLarge
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
            } else if line.hasPrefix("BVAGENT CLIPSYNC skip bytes="), line.contains(" too-large ") {
                show(.macTextTooLarge, copy: Self.copy(in: line))
            } else if line.hasPrefix("BVAGENT CLIPSYNC refused guest-text too-large ") {
                show(.windowsTextTooLarge, copy: Self.copy(in: line))
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
    /// Without copy numbers every sync counts as later.
    private func clear(unlessAbout lineCopy: Int?) {
        if let lineCopy, let copy, lineCopy <= copy { return }
        reset()
    }

    var displayed: (text: String, help: String, symbol: String)? {
        let imageHelp = "Clipboard PNGs can be up to 32 MiB, with each side up to 16,384 pixels. You can still paste the original where you copied it."
        let textHelp = "Copy the text in smaller sections, or save it as a file and move it through a shared folder."
        let limits = "Each copy can contain up to \(fileLimits.files) items and \(Self.size(fileLimits.bytes)) in total."
        let windowsFilesHelp = "\(limits) Use a shared folder for larger selections, up to 16 GiB per file."
        let macFilesHelp = "\(limits) Drag the selection onto the VM window to copy it to the Windows desktop."
        switch shown {
        case .macTextTooLarge:
            return ("The text copied on your Mac is too large to send to Windows.", textHelp, "doc.plaintext")
        case .windowsTextTooLarge:
            return ("The text copied in Windows is too large to send to your Mac.", textHelp, "doc.plaintext")
        case .macImageTooLarge:
            return ("The image copied on your Mac is too large to send to Windows.", imageHelp, "photo")
        case .windowsImageTooLarge:
            return ("The image copied in Windows is too large to send to your Mac.", imageHelp, "photo")
        case .macFilesTooLarge:
            return ("The selection exceeds the clipboard limits. Only the file names were sent to Windows.", macFilesHelp, "doc.on.doc")
        case .windowsFilesTooLarge:
            return ("The Windows selection exceeds the clipboard limits for copying files to your Mac.", windowsFilesHelp, "doc.on.doc")
        case .macFilesUnreadable:
            return ("Your Mac couldn’t read the full selection. Only the file names were sent to Windows.",
                    "Check that you can read each file and folder, then copy them again.", "doc.on.doc")
        case .windowsFilesLowSpace:
            return ("Your Mac doesn’t have enough space for the files copied in Windows.",
                    "Free up enough space on your Mac for the files plus 2 GiB, then copy them again.", "doc.on.doc")
        case .macFilesGuestFull:
            return ("Windows doesn’t have enough space for the files copied on your Mac.",
                    "Free up space in Windows, then copy the files again.", "doc.on.doc")
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

    private static func size(_ bytes: Int64) -> String {
        if bytes > 0 && bytes % (1 << 30) == 0 { return "\(bytes >> 30) GiB" }
        if bytes > 0 && bytes % (1 << 20) == 0 { return "\(bytes >> 20) MiB" }
        return bytes == 1 ? "1 byte" : "\(bytes) bytes"
    }
}

struct HvfClipboardImageNoticeView: View {
    @ObservedObject var notice: HvfClipboardImageNotice

    var body: some View {
        if let shown = notice.displayed {
            VStack(alignment: .leading, spacing: 4) {
                Label {
                    Text(shown.text)
                } icon: {
                    Image(systemName: shown.symbol).foregroundStyle(.orange)
                }
                .font(.callout)
                Text(shown.help)
                    .font(.caption).foregroundStyle(.secondary)
            }
            .textSelection(.enabled)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("bridgevm.runtime.clipboard.image.notice")
        }
    }
}
