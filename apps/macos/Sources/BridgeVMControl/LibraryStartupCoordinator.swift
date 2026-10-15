import Foundation
import Combine

/// One accepted scan owns its lease until acknowledgement, even if its UI waiter leaves.
@MainActor
final class LibraryStartupCoordinator: ObservableObject {
    @Published private(set) var model: LibraryModel?
    @Published private(set) var failure: String?
    private let scan: @Sendable () throws -> VMLibraryScan
    private let validateOwner: () throws -> Void
    private let publish: (VMLibraryScan) throws -> LibraryModel
    private let closeOwner: () -> Void
    private var task: Task<Void, Never>?
    private var token: UUID?
    private var closed = false
    private var ownerClosed = false

    init(scan: @escaping @Sendable () throws -> VMLibraryScan,
         validateOwner: @escaping () throws -> Void,
         publish: @escaping (VMLibraryScan) throws -> LibraryModel,
         closeOwner: @escaping () -> Void) {
        self.scan = scan; self.validateOwner = validateOwner
        self.publish = publish; self.closeOwner = closeOwner
    }

    func load() async {
        guard !closed, model == nil else { return }
        if let task { await task.value; return }
        do { try validateOwner() }
        catch { failure = error.localizedDescription; return }
        failure = nil
        let accepted = UUID(), scan = self.scan
        token = accepted
        let task = Task { [self] in
            let result = await Task.detached(priority: .userInitiated) { Result { try scan() } }.value
            finish(result, token: accepted)
        }
        self.task = task
        await task.value
    }

    func close() {
        closed = true
        token = nil
        if task == nil { releaseOwner() }
    }

    private func finish(_ result: Result<VMLibraryScan, Error>, token: UUID) {
        defer {
            task = nil
            if closed { releaseOwner() }
        }
        guard !closed, self.token == token else { return }
        do {
            try validateOwner()
            model = try publish(result.get())
        } catch { failure = error.localizedDescription }
    }

    private func releaseOwner() {
        guard !ownerClosed else { return }
        ownerClosed = true
        closeOwner()
    }
}
