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

    static let staleMessage = "설치 준비 정보가 변경되었습니다. 다시 확인한 뒤 설치를 시작하세요."
    static let missingMessage = "이 VM의 설치 대상을 찾을 수 없습니다. 목록을 새로고침하고 VM을 다시 선택하세요."
    static let ownerMessage = "VM 라이브러리를 사용할 수 없습니다. 라이브러리 화면에서 다시 시도하세요."

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
