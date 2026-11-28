import Combine
import Foundation

/// One session's Finder drops: staged, handed to the runtime in order, then followed through the
/// runtime's `BVAGENT DROP start|progress|done|failed <id>` lines (agent_console/file_drop.rs) to
/// a short notice. The runtime runs one drop at a time; a second one waits behind the first.
@MainActor
final class HvfFileDropQueue: ObservableObject {
    struct Notice: Equatable {
        let text: String
        let failed: Bool
    }

    static let notRunningText = "Windows isn’t running, so the files weren’t copied."

    @Published private(set) var notice: Notice?
    /// How long a finished drop's notice stays up.
    var finishedNoticeSeconds: TimeInterval = 8

    private struct Job {
        let id: String
        let staging: URL
        /// Open each item in Windows once copied (Finder's Open With, a drop on the Dock icon).
        let open: Bool
        let itemName: String?
        var skipped = 0
        var unreadable = 0
        /// Staged while Windows was still starting: sent at the agent's SERVICE start.
        var waiting: HvfFileDropStaging.Summary?
        var sent = false
        var started = false
        var files = (done: 0, total: 0)
        var bytes = (done: Int64(0), total: Int64(0))
    }

    private var earlierIssue: (message: String, count: Int)?
    private var jobs: [Job] = []
    private var noticeDismissal: Task<Void, Never>?

    func isPending(_ id: String) -> Bool { jobs.contains { $0.id == id } }

    func refuse(_ text: String) {
        notice = Notice(text: text, failed: true)
    }

    /// The close button on a failed notice. A finished drop's notice goes by itself.
    func dismiss() {
        guard notice?.failed == true else { return }
        notice = nil
    }

    /// A new job and its staging folder below `root`. With no other job in flight, whatever an
    /// earlier run left in `root` goes first.
    func begin(in root: URL, open: Bool = false, itemName: String? = nil) -> (id: String, staging: URL) {
        if jobs.isEmpty {
            earlierIssue = nil
            Self.discard(root, root: root)
            Self.sweep(beside: root)
        }
        let id = UUID().uuidString
        let staging = root.appendingPathComponent(id, isDirectory: true)
        jobs.append(Job(id: id, staging: staging, open: open, itemName: itemName))
        show()
        return (id, staging)
    }

    func staged(_ id: String, _ summary: HvfFileDropStaging.Summary, sent: Bool) {
        guard let index = jobs.firstIndex(where: { $0.id == id }) else { return }
        guard sent else {
            return finish(index, Notice(text: "Couldn’t start the copy. Try again in a moment.", failed: true))
        }
        jobs[index].sent = true
        jobs[index].waiting = nil
        jobs[index].skipped = summary.skipped
        jobs[index].unreadable = summary.unreadable
        jobs[index].files.total = summary.files
        jobs[index].bytes.total = summary.bytes
        show()
    }

    /// Staged before Windows can take the files: they wait for `waitingForWindows()`.
    func waitForWindows(_ id: String, _ summary: HvfFileDropStaging.Summary) {
        guard let index = jobs.firstIndex(where: { $0.id == id }) else { return }
        jobs[index].waiting = summary
        show()
    }

    /// The drops waiting for Windows, in drop order. Each one then gets `staged(_:_:sent:)`.
    func waitingForWindows() -> [(id: String, staging: URL, open: Bool, summary: HvfFileDropStaging.Summary)] {
        jobs.compactMap { job in job.waiting.map { (job.id, job.staging, job.open, $0) } }
    }

    func nothingToCopy(_ id: String, unreadable: Bool = false) {
        guard let index = jobs.firstIndex(where: { $0.id == id }) else { return }
        finish(index, Notice(text: unreadable
            ? "Nothing was copied because the Mac isn’t allowed to read these items. Check their permissions in Finder."
            : "Nothing to copy. Choose regular files or folders instead of symbolic links or special files.", failed: true))
    }

    func stagingFailed(_ id: String) {
        guard let index = jobs.firstIndex(where: { $0.id == id }) else { return }
        finish(index, Notice(text: "Couldn’t prepare the files. Check their permissions and the free space on your Mac.", failed: true))
    }

    func consume(_ event: BvAgentEvent) {
        guard case let .unknown(raw) = event else { return }
        let line = raw.hasSuffix("\r") ? String(raw.dropLast()) : raw
        if HvfGuestResetRecord.matches(line) {
            return abandon("Windows restarted before the copy finished. Try again.")
        }
        let fields = line.split(separator: " ").map(String.init)
        guard fields.count >= 4, fields[0] == "BVAGENT", fields[1] == "DROP",
              let index = jobs.firstIndex(where: { $0.id == fields[3] && $0.sent }) else { return }
        let values = Dictionary(fields.compactMap { field -> (String, String)? in
            let pair = field.split(separator: "=", maxSplits: 1).map(String.init)
            return pair.count == 2 ? (pair[0], pair[1]) : nil
        }, uniquingKeysWith: { _, last in last })
        // start/done carry totals (`files=N`), progress/failed carry `files=done/total`.
        let files = values["files"].flatMap(Self.counts)
        let bytes = values["bytes"].flatMap(Self.counts)
        if let files { jobs[index].files = (Int(files.done ?? (fields[2] == "done" ? files.total : 0)), Int(files.total)) }
        if let bytes { jobs[index].bytes = (bytes.done ?? (fields[2] == "done" ? bytes.total : 0), bytes.total) }
        switch fields[2] {
        case "start", "progress":
            jobs[index].started = true
            show()
        case "done":
            let job = jobs[index]
            let copied = "Copied to the Windows desktop (\(Self.quantity(job.files.done, "file")), \(Self.size(job.bytes.done)))."
            var done = Notice(text: copied, failed: false)
            // 'opened' counts selected items, which may include folders with multiple files.
            if job.open, let opened = values["opened"] {
                if opened == "old-agent" {
                    done = Notice(text: "\(copied) This version of Guest Tools can’t open items automatically.", failed: true)
                } else if let counts = Self.counts(opened), let ok = counts.done {
                    done = ok < counts.total
                        ? Notice(text: "\(copied) Open \(Self.quantity(counts.total - ok, "remaining item")) from the desktop.", failed: true)
                        : Notice(text: "\(copied) Opened \(Self.quantity(ok, "selected item")).", failed: false)
                }
            }
            let skipped = job.skipped > 0 ? " Skipped \(Self.quantity(job.skipped, "unsupported item")) (symbolic links or special files)." : ""
            // Not copied though the user chose them: the notice stays until it is closed.
            let unreadable = job.unreadable > 0 ? " Skipped \(Self.quantity(job.unreadable, "item")) the Mac isn’t allowed to read." : ""
            finish(index, Notice(text: done.text + skipped + unreadable, failed: done.failed || job.unreadable > 0))
        case "failed":
            let job = jobs[index]
            let reason = fields.count > 4 ? fields[4] : ""
            let partial = job.files.done > 0 ? " Copied \(job.files.done) of \(Self.quantity(job.files.total, "file"))." : ""
            finish(index, Notice(text: Self.failureText(reason) + partial, failed: true))
        default:
            break
        }
    }

    /// The VM stopped: nothing is in flight any more, and no notice outlives the run.
    func reset() {
        for job in jobs { Self.discard(job.staging) }
        jobs = []
        earlierIssue = nil
        if notice != nil { notice = nil }
    }

    /// A staged tree, or the whole drops folder, is renamed aside next to that folder at once and deleted
    /// off the main actor: 20,000 cloned files took 0.6 s to delete. Whatever a quit or crash leaves
    /// aside goes with the next first drop (sweep).
    /// `url` is a job's staging folder inside `root` (the drops folder), or `root` itself.
    nonisolated static func discard(_ url: URL, root: URL? = nil) {
        let root = root ?? url.deletingLastPathComponent()
        let aside = root.deletingLastPathComponent().appendingPathComponent("\(root.lastPathComponent).\(UUID().uuidString).discarded")
        guard rename(url.path, aside.path) == 0 else {
            if errno != ENOENT { try? FileManager.default.removeItem(at: url) }
            return
        }
        Task.detached(priority: .utility) { try? FileManager.default.removeItem(at: aside) }
    }

    nonisolated static func sweep(beside root: URL) {
        let parent = root.deletingLastPathComponent(), prefix = root.lastPathComponent + "."
        Task.detached(priority: .utility) {
            for name in (try? FileManager.default.contentsOfDirectory(atPath: parent.path)) ?? []
            where name.hasPrefix(prefix) && name.hasSuffix(".discarded") {
                try? FileManager.default.removeItem(at: parent.appendingPathComponent(name))
            }
        }
    }

    static func failureText(_ reason: String) -> String {
        switch reason {
        case "agent-too-old":
            // BridgeVM updates Guest Tools itself about 20 s after it starts in Windows, once per app
            // launch (HvfAgentUpdate.swift): a drop sent before that, or after a failed update, gets this.
            return "This version of Guest Tools can’t receive files. Quit and reopen BridgeVM, then start Windows to update it."
        case "guest-error":
            return "Windows couldn’t save the files. Check free disk space and write access to the Windows desktop."
        case "stalled", "agent-restarted":
            return "Guest Tools disconnected during the copy. Try again after it reconnects."
        case "read-error", "staging-unreadable":
            return "The copy stopped because the Mac couldn’t read a file. Check its permissions and try again."
        default:
            return "Couldn’t copy the files. Try again."
        }
    }

    private func abandon(_ text: String) {
        guard jobs.contains(where: \.sent) else { return }
        for job in jobs where job.sent { Self.discard(job.staging) }
        jobs.removeAll(where: \.sent)
        notice = Notice(text: text, failed: true)
    }

    private func finish(_ index: Int, _ result: Notice) {
        let job = jobs.remove(at: index)
        Self.discard(job.staging)
        guard jobs.isEmpty else {
            if result.failed {
                if earlierIssue != nil {
                    earlierIssue?.count += 1
                } else {
                    let context = job.itemName.map { "The earlier transfer of “\($0)” needs attention." }
                        ?? "An earlier transfer needs attention."
                    earlierIssue = ("\(context) \(result.text)", 1)
                }
            }
            return show()
        }

        var final = result
        if let issue = earlierIssue {
            let others = issue.count > 1 ? " Review \(Self.quantity(issue.count - 1, "other transfer")) as well." : ""
            final = Notice(text: "\(result.text)\n\(issue.message)\(others)", failed: true)
            earlierIssue = nil
        }
        notice = final
        noticeDismissal?.cancel()
        guard !final.failed else { return }
        let seconds = finishedNoticeSeconds
        noticeDismissal = Task { [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(seconds * 1_000_000_000))
            guard !Task.isCancelled, let self, self.jobs.isEmpty, self.notice == final else { return }
            self.notice = nil
        }
    }

    private func show() {
        guard let job = jobs.first else { return }
        let waiting = jobs.count > 1 ? " · \(jobs.count - 1) queued" : ""
        let text: String
        if job.waiting != nil {
            text = "The copy starts when Windows is ready…"
        } else if !job.sent {
            text = "Preparing to copy…"
        } else if !job.started {
            text = "Starting the copy to Windows…"
        } else {
            text = "Copying to Windows · \(job.files.done) of \(Self.quantity(job.files.total, "file")) · "
                + "\(Self.size(job.bytes.done)) / \(Self.size(job.bytes.total))"
        }
        notice = Notice(text: text + waiting, failed: false)
    }

    /// `done/total`, or a bare total.
    private static func counts(_ value: String) -> (done: Int64?, total: Int64)? {
        let parts = value.split(separator: "/").map { Int64($0) }
        switch parts.count {
        case 1: return parts[0].map { (nil, $0) }
        case 2: return parts[0].flatMap { done in parts[1].map { (done, $0) } }
        default: return nil
        }
    }

    private static func quantity(_ count: some BinaryInteger, _ noun: String) -> String {
        "\(count) \(noun)\(count == 1 ? "" : "s")"
    }

    private static func size(_ bytes: Int64) -> String {
        if bytes == 0 { return "0 B" }
        return ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file)
    }
}
