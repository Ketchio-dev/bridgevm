import Foundation

/// The runtime stops sharing for the rest of a boot generation on these exact lines; a stop
/// outranks a root deferral in the notice. Wording promises nothing beyond this run.
enum HvfShareStop: Equatable {
    case agentTooOld, failedDestinations

    init?(line: String) {
        switch line {
        case "BVAGENT SHARE agent-too-old (needs v3-share2); share disabled": self = .agentTooOld
        case "BVAGENT SHARE disabled failed-destination-limit": self = .failedDestinations
        default: return nil
        }
    }
}

extension HvfShareRootNotice {
    var displayed: (text: String, help: String)? {
        if launchUnavailable { return (HvfEngineConfig.shareUnavailableMessage, "") }
        switch stopped {
        case .agentTooOld:
            return ("This version of Guest Tools can’t sync shared folders.",
                    "You can keep using Windows without the shared folder.")
        case .failedDestinations:
            return ("Folder sync stopped for this session after repeated write failures on your Mac.",
                    "Check the folder’s write permissions and available disk space, then restart the VM.")
        case nil:
            if let paused {
                let limit = paused.limit.formatted()
                switch paused.reason {
                case .tooManyEntries:
                    return ("Folder sync paused because the Windows folder has more than \(limit) items.",
                            "Move some items out of the Windows folder, or shut down the VM and choose a smaller shared folder.")
                case .tooManyBytes:
                    return ("Folder sync paused because the Windows folder has too many files with long names.",
                            "Shorten some file names or move items out of the Windows folder to resume syncing.")
                }
            }
            if let missingFolder {
                return ("The shared folder “\(missingFolder)” was unavailable when Windows started.",
                        "Reconnect the folder to your Mac. Sharing will resume automatically.")
            }
            if isBlocked {
                return ("Some changes are waiting because BridgeVM can’t verify the shared folder.",
                        "Reconnect the original folder, or shut down the VM and choose another folder.")
            }
            if let lowSpace {
                let name = "“\(lowSpace.last)”"
                let others = lowSpace.count - 1
                let files = others == 0 ? name : "\(name) and \(others) other \(others == 1 ? "file" : "files")"
                return ("Your Mac needs more space to receive \(files) from Windows.",
                        "Free up space on your Mac. Sync will retry automatically.")
            }
            guard let tooLarge else { return nil }
            let shareLimit = HvfEngineConfig.shareMaxKiB * 1024, limit = tooLarge.limit ?? shareLimit
            let text = "Sharing skipped “\(tooLarge.last)” because the per-file limit is \(HvfShareTooLarge.sizeText(limit))."
            let total = tooLarge.count > 1 ? " Folder sharing has skipped oversized files \(tooLarge.count) times this session." : ""
            let help = limit < shareLimit
                ? "Larger files sync after BridgeVM updates Guest Tools in Windows, which it does automatically."
                : tooLarge.fromWindows ? "To share a larger file from Windows, split or compress it first."
                : "To copy a large file from your Mac to Windows, drag it onto the VM window."
            return (text + total, help)
        }
    }

}

/// The guest folder's listing is over the runtime's limit (`BVAGENT SHARE paused
/// too-many-entries limit=<entries>`, or `too-many-bytes limit=<bytes>`): without a listing the
/// runtime cannot tell a newer Windows edit, so it sends, deletes and fetches nothing until
/// `BVAGENT SHARE resumed` (a listing within the limit).
struct HvfSharePaused: Equatable {
    enum Reason: Equatable { case tooManyEntries, tooManyBytes }
    let reason: Reason
    let limit: Int

    init(reason: Reason, limit: Int) {
        self.reason = reason
        self.limit = limit
    }

    init?(line: String) {
        let prefix = "BVAGENT SHARE paused "
        guard line.hasPrefix(prefix) else { return nil }
        let words = line.dropFirst(prefix.count).split(separator: " ", omittingEmptySubsequences: false)
        // Words after the limit are ignored (a later runtime may add some).
        guard words.count >= 2, words[1].hasPrefix("limit=") else { return nil }
        let digits = words[1].dropFirst("limit=".count)
        guard !digits.isEmpty, digits.utf8.allSatisfy({ $0 >= 0x30 && $0 <= 0x39 }),
              let limit = Int(digits), limit > 0 else { return nil }
        switch words[0] {
        case "too-many-entries": reason = .tooManyEntries
        case "too-many-bytes": reason = .tooManyBytes
        default: return nil
        }
        self.limit = limit
    }
}

/// A shared-folder file left out for its size (`BVAGENT SHARE skip <name> too-large <bytes>
/// limit=<bytes> mac|windows`, share.rs `too_large_line`). The limit is 64 MiB until Guest Tools move
/// files in pieces; an older runtime's line ends at the size and meant the share's own limit.
struct HvfShareTooLarge: Equatable {
    let count: Int
    let last: String
    var limit: Int? = nil
    var fromWindows = false

    typealias Skip = (name: String, limit: Int?, fromWindows: Bool)

    static func skip(in line: String) -> Skip? {
        for side in [" mac", " windows"] where line.hasSuffix(side) {
            let head = line.dropLast(side.count)
            guard let marker = head.range(of: " limit=", options: .backwards) else { break }
            let digits = head[marker.upperBound...]
            guard !digits.isEmpty, digits.utf8.allSatisfy({ $0 >= 0x30 && $0 <= 0x39 }), let limit = Int(digits),
                  let name = hvfShareSkippedName(in: String(head[..<marker.lowerBound]), reason: "too-large")
            else { break }
            return (name, limit, side == " windows")
        }
        return hvfShareSkippedName(in: line, reason: "too-large").map { ($0, nil, false) }
    }

    static func sizeText(_ bytes: Int) -> String {
        if bytes % (1 << 30) == 0 { return "\(bytes >> 30) GiB" }
        if bytes % (1 << 20) == 0 { return "\(bytes >> 20) MiB" }
        return "\(bytes.formatted()) bytes"
    }
}

/// Windows files the Mac had no room for and that have not reached it since
/// (`BVAGENT SHARE skip <name> low-space <bytes>`): a large file needs its size plus 2 GiB free,
/// and any write the Mac refuses for want of space is reported too. The runtime asks again at each
/// guest listing and reports each version once; a file counts once until it arrives.
struct HvfShareLowSpace: Equatable {
    /// Each waiting file, and when it was last reported (a running number).
    private var waiting: [String: Int]
    private var reports = 0
    /// The most recently reported file still waiting.
    private(set) var last: String
    var count: Int { waiting.count }

    init(_ name: String) {
        waiting = [name: 0]
        last = name
    }

    static func name(in line: String) -> String? { hvfShareSkippedName(in: line, reason: "low-space") }

    func reported(_ name: String) -> Self {
        var next = self
        next.reports += 1
        next.waiting[name] = next.reports
        next.last = name
        return next
    }

    func isWaiting(for name: String) -> Bool { waiting[name] != nil }

    /// Without `name`, which reached the Mac; nil once no file is left waiting.
    func arrived(_ name: String) -> Self? {
        var next = self
        next.waiting[name] = nil
        guard !next.waiting.isEmpty else { return nil }
        if name == last, let newest = next.waiting.max(by: { $0.value < $1.value }) {
            next.last = newest.key
        }
        return next
    }
}

/// The name in `BVAGENT SHARE skip <name> <reason> <ASCII digits>`.
private func hvfShareSkippedName(in line: String, reason: String) -> String? {
    let prefix = "BVAGENT SHARE skip "
    guard line.hasPrefix(prefix) else { return nil }
    // Only after the prefix: a line that is all marker must not slice backwards.
    let rest = line.dropFirst(prefix.count)
    guard let marker = rest.range(of: " \(reason) ", options: .backwards) else { return nil }
    let size = rest[marker.upperBound...]
    guard !size.isEmpty, size.utf8.allSatisfy({ $0 >= 0x30 && $0 <= 0x39 }) else { return nil }
    let name = rest[..<marker.lowerBound]
    return name.isEmpty ? nil : String(name)
}
