import Combine
import Foundation

@MainActor
final class HvfRuntimeReadinessModel: ObservableObject {
    typealias Reader = @Sendable (HvfEngineConfig, URL) -> HvfWindowsReadinessReport
    private struct Input: Equatable {
        let configuration: HvfEngineConfig
        let repoRoot: URL
    }
    private struct Request {
        let id = UUID()
        let input: Input
    }
    @Published private var notificationID = UUID()
    private let reader: Reader
    private var wanted: Request?
    private var completed: (request: Request, report: HvfWindowsReadinessReport)?
    private var task: Task<Void, Never>?
    var isChecking: Bool { task != nil }

    init(reader: @escaping Reader = { $0.readiness(repoRoot: $1) }) { self.reader = reader }

    func request(configuration: HvfEngineConfig, repoRoot: URL, force: Bool = false) {
        let input = Input(configuration: configuration, repoRoot: repoRoot)
        guard force || wanted?.input != input else { return }
        wanted = Request(input: input)
        completed = nil
        if task == nil { task = Task { await drain() } }
        // Commit before callbacks: reentrant requests see the retained worker.
        notificationID = UUID()
    }

    func report(configuration: HvfEngineConfig, repoRoot: URL) -> HvfWindowsReadinessReport? {
        let input = Input(configuration: configuration, repoRoot: repoRoot)
        guard task == nil, let completed, completed.request.id == wanted?.id,
              completed.request.input == input else { return nil }
        return completed.report
    }

    private func drain() async {
        while let request = wanted {
            let reader = reader, input = request.input
            let report = await Task.detached(priority: .userInitiated) {
                reader(input.configuration, input.repoRoot)
            }.value
            guard wanted?.id == request.id else { continue }
            completed = (request, report)
            task = nil
            // No domain mutation follows: a callback may reserve a new read.
            notificationID = UUID()
            return
        }
    }
}
