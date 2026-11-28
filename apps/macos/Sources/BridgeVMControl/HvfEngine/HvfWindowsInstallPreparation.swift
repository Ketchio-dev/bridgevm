import Combine
import Foundation

@MainActor
final class HvfWindowsInstallPreparation: ObservableObject {
    enum State {
        case preparing
        case ready(HvfWindowsInstallSession)
        case failed(String)
    }

    enum Lookup: Equatable {
        case input(HvfWindowsInstallPlanInput)
        case ready(ObjectIdentifier)
        case unavailable(VMConfig?)
    }

    static let staleMessage = "The installation settings changed. Check them again before starting."
    static let missingMessage = "Couldn’t find this VM. Refresh the library, then select the VM again."
    static let ownerMessage = "The VM library is unavailable. Return to the library and try again."

    let slug: String
    @Published private(set) var state: State
    private(set) var lookup: Lookup
    private(set) var attemptID = UUID()
    private(set) var preparationTask: Task<Void, Never>?
    var retryAction: (() -> Void)?

    init(slug: String, lookup: Lookup, state: State) {
        self.slug = slug
        self.lookup = lookup
        self.state = state
    }

    func retry() { retryAction?() }

    func begin(_ input: HvfWindowsInstallPlanInput) -> UUID {
        attemptID = UUID()
        lookup = .input(input)
        preparationTask = nil
        state = .preparing
        return attemptID
    }

    func attach(_ task: Task<Void, Never>, attempt: UUID) {
        guard attemptID == attempt else { return }
        preparationTask = task
    }

    func complete(_ result: State, attempt: UUID) {
        guard attemptID == attempt else { return }
        state = result
    }

    func invalidate(_ message: String) {
        attemptID = UUID()
        state = .failed(message)
    }

    func adopt(_ session: HvfWindowsInstallSession) {
        attemptID = UUID()
        lookup = .ready(ObjectIdentifier(session))
        state = .ready(session)
    }
}
