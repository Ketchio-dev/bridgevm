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

    static let notReadyText = "게스트 에이전트가 아직 준비되지 않아 파일을 복사하지 않았습니다. Windows가 시작된 뒤 다시 끌어다 놓으세요."

    @Published private(set) var notice: Notice?
    /// How long a finished drop's notice stays up.
    var finishedNoticeSeconds: TimeInterval = 8

    private struct Job {
        let id: String
        let staging: URL
        var skipped = 0
        var sent = false
        var started = false
        var files = (done: 0, total: 0)
        var bytes = (done: Int64(0), total: Int64(0))
    }

    private var jobs: [Job] = []

    func refuse(_ text: String) {
        notice = Notice(text: text, failed: true)
    }

    /// A new job and its staging folder below `root`. With no other job in flight, whatever an
    /// earlier run left in `root` goes first.
    func begin(in root: URL) -> (id: String, staging: URL) {
        if jobs.isEmpty { try? FileManager.default.removeItem(at: root) }
        let id = UUID().uuidString
        let staging = root.appendingPathComponent(id, isDirectory: true)
        jobs.append(Job(id: id, staging: staging))
        show()
        return (id, staging)
    }

    func staged(_ id: String, _ summary: HvfFileDropStaging.Summary, sent: Bool) {
        guard let index = jobs.firstIndex(where: { $0.id == id }) else { return }
        guard sent else {
            return finish(index, Notice(text: "게스트 에이전트에 복사를 요청하지 못했습니다. 잠시 뒤 다시 끌어다 놓으세요.", failed: true))
        }
        jobs[index].sent = true
        jobs[index].skipped = summary.skipped
        jobs[index].files.total = summary.files
        jobs[index].bytes.total = summary.bytes
        show()
    }

    func nothingToCopy(_ id: String) {
        guard let index = jobs.firstIndex(where: { $0.id == id }) else { return }
        finish(index, Notice(text: "복사할 파일이 없습니다. 바로가기(심볼릭 링크)와 특수 파일은 복사하지 않습니다.", failed: true))
    }

    func stagingFailed(_ id: String) {
        guard let index = jobs.firstIndex(where: { $0.id == id }) else { return }
        finish(index, Notice(text: "Mac에서 파일을 읽거나 복사 준비를 하지 못해 복사하지 않았습니다. 파일 권한과 Mac의 남은 공간을 확인하세요.", failed: true))
    }

    func consume(_ event: BvAgentEvent) {
        guard case let .unknown(raw) = event else { return }
        let line = raw.hasSuffix("\r") ? String(raw.dropLast()) : raw
        if HvfGuestResetRecord.matches(line) {
            return abandon("Windows가 다시 시작되어 파일 복사가 중단되었습니다. 다시 끌어다 놓으세요.")
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
            var text = "파일 \(job.files.done)개(\(Self.size(job.bytes.done)))를 Windows 바탕 화면에 복사했습니다."
            if job.skipped > 0 { text += " 바로가기와 특수 파일 \(job.skipped)개는 건너뛰었습니다." }
            finish(index, Notice(text: text, failed: false))
        case "failed":
            let job = jobs[index]
            let reason = fields.count > 4 ? fields[4] : ""
            let partial = job.files.done > 0 ? " (파일 \(job.files.done)/\(job.files.total)개는 복사됨)" : ""
            finish(index, Notice(text: Self.failureText(reason) + partial, failed: true))
        default:
            break
        }
    }

    /// The VM stopped: nothing is in flight any more, and no notice outlives the run.
    func reset() {
        for job in jobs { try? FileManager.default.removeItem(at: job.staging) }
        jobs = []
        if notice != nil { notice = nil }
    }

    static func failureText(_ reason: String) -> String {
        switch reason {
        case "agent-too-old":
            return "이 VM의 게스트 에이전트가 파일 끌어다 놓기를 지원하지 않아 복사하지 않았습니다. 게스트 에이전트를 업데이트하세요."
        case "guest-error":
            return "Windows에 파일을 쓰지 못해 복사를 멈췄습니다. Windows 바탕 화면의 남은 공간과 권한을 확인하세요."
        case "stalled", "agent-restarted":
            return "게스트 에이전트와의 연결이 끊겨 복사를 멈췄습니다. 다시 끌어다 놓으세요."
        case "read-error", "staging-unreadable":
            return "Mac에서 복사할 파일을 읽지 못해 복사를 멈췄습니다."
        default:
            return "파일 복사에 실패했습니다. 다시 끌어다 놓으세요."
        }
    }

    private func abandon(_ text: String) {
        guard jobs.contains(where: \.sent) else { return }
        for job in jobs where job.sent { try? FileManager.default.removeItem(at: job.staging) }
        jobs.removeAll(where: \.sent)
        notice = Notice(text: text, failed: true)
    }

    private func finish(_ index: Int, _ final: Notice) {
        try? FileManager.default.removeItem(at: jobs[index].staging)
        jobs.remove(at: index)
        guard jobs.isEmpty else { return show() }
        notice = final
        guard !final.failed else { return }
        let seconds = finishedNoticeSeconds
        Task { [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(seconds * 1_000_000_000))
            guard let self, self.jobs.isEmpty, self.notice == final else { return }
            self.notice = nil
        }
    }

    private func show() {
        guard let job = jobs.first else { return }
        let waiting = jobs.count > 1 ? " · 대기 \(jobs.count - 1)건" : ""
        let text: String
        if !job.sent {
            text = "끌어다 놓은 항목을 복사할 준비를 하고 있습니다…"
        } else if !job.started {
            text = "Windows로 복사를 시작하고 있습니다…"
        } else {
            text = "Windows로 복사 중: 파일 \(job.files.done)/\(job.files.total)개 · "
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

    private static func size(_ bytes: Int64) -> String {
        ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file)
    }
}
