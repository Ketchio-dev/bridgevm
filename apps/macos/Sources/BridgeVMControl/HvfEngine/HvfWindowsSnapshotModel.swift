import Foundation

/// The powered-off snapshot card (HvfWindowsSnapshotCard). A VM keeps one snapshot: restoring it
/// throws away every disk change made since, and creating one replaces it, so both ask first while
/// a snapshot exists and run nothing until the user agrees. A failure reads as one Korean sentence;
/// the technical detail is in the event feed (HvfEngineSession.runSnapshot).
@MainActor
final class HvfWindowsSnapshotModel: ObservableObject {
    typealias Operation = HvfWindowsSnapshotCommand.Operation
    typealias Run = @MainActor (Operation) async throws -> Void

    /// The question before an operation that throws away the snapshot or the changes since it.
    struct Confirmation {
        let operation: Operation
        let date: Date
        let run: Run

        var title: String {
            let when = HvfWindowsSnapshotModel.dateText(date)
            return operation == .restore ? "\(when)의 스냅샷으로 되돌릴까요?" : "\(when)의 스냅샷을 새 스냅샷으로 바꿀까요?"
        }
        var message: String {
            operation == .restore ? "그 뒤에 바뀐 디스크 내용은 모두 사라집니다." : "스냅샷은 하나만 보관되어 지금 스냅샷은 지워집니다."
        }
        var action: String { operation == .restore ? "되돌리기" : "바꾸기" }
    }

    nonisolated static let idleText = "VM을 종료한 뒤 디스크와 UEFI vars를 한 쌍으로 저장할 수 있습니다."
    @Published private(set) var confirmation: Confirmation?
    @Published private(set) var busy = false
    @Published private(set) var status = idleText
    @Published private(set) var failed = false

    /// Asks first when `snapshotDate` names the snapshot this would replace or restore; creates at
    /// once when there is none. Returns the started work, if it started.
    @discardableResult
    func request(_ operation: Operation, snapshotDate: Date?, run: @escaping Run) -> Task<Void, Never>? {
        guard !busy else { return nil }
        if let snapshotDate {
            confirmation = Confirmation(operation: operation, date: snapshotDate, run: run)
            return nil
        }
        guard operation == .create else {
            status = Self.noSnapshot; failed = true
            return nil
        }
        return start(operation, run)
    }

    /// The user agreed to `confirmation`, the one the dialog showed.
    @discardableResult
    func confirm(_ confirmation: Confirmation) -> Task<Void, Never>? {
        self.confirmation = nil
        guard !busy else { return nil }
        return start(confirmation.operation, confirmation.run)
    }

    func cancel() { confirmation = nil }

    private func start(_ operation: Operation, _ run: @escaping Run) -> Task<Void, Never> {
        busy = true; failed = false
        status = operation == .create ? "스냅샷 생성 중…" : "스냅샷 복원 중…"
        return Task {
            do {
                try await run(operation)
                status = operation == .create ? "스냅샷 생성 완료" : "스냅샷 복원 완료"
            } catch {
                status = Self.message(for: error, operation: operation); failed = true
            }
            busy = false
        }
    }

    /// "2026년 11월 11일 오후 3:42"
    nonisolated static func dateText(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "ko_KR")
        formatter.dateStyle = .long
        formatter.timeStyle = .short
        return formatter.string(from: date)
    }

    nonisolated static let noSnapshot = "되돌릴 스냅샷이 없으니 먼저 스냅샷을 만드세요."

    /// One Korean sentence: what happened and what to do. The helper's English (the Rust
    /// SnapshotError and io::Error texts) is matched for the failures that happen in practice;
    /// anything else gets the operation's general sentence.
    nonisolated static func message(for error: Error, operation: Operation) -> String {
        let error = error as NSError
        let failed = operation == .create ? "스냅샷을 만들지 못했으니" : "스냅샷으로 되돌리지 못했으니"
        let text = error.localizedDescription
        var failure: HvfWindowsSnapshotCommand.Failure?
        switch error.domain {
        case HvfWindowsSnapshotCommand.errorDomain: failure = .init(rawValue: error.code)
        case "BridgeVM.RuntimeMutation": failure = .notStopped
        default: break
        }
        if text.contains("media is in use") { failure = .notStopped } // the helper's writer-lease refusal
        switch failure {
        case .notStopped?:
            return "VM이 실행 중이거나 다른 작업 중이라 \(failed) VM을 완전히 종료한 뒤 다시 시도하세요."
        case .settingsUnsaved?:
            return "이 화면에서 바꾼 설정이 아직 적용되지 않아 \(failed) 바꾼 내용을 되돌리거나 VM을 한 번 시작해 적용한 뒤 다시 시도하세요."
        case .notLibraryVM?:
            return "스냅샷은 BridgeVM 라이브러리에 있는 VM에서만 쓸 수 있으니 라이브러리에서 VM을 열어 다시 시도하세요."
        case .helperMissing?:
            return "스냅샷 도우미 프로그램을 찾지 못해 \(failed) BridgeVM을 다시 설치하세요."
        case .unsafeFolder?:
            return "VM 번들 안의 스냅샷 폴더를 안전하게 쓸 수 없어 \(failed) 그 폴더가 심볼릭 링크가 아닌 일반 폴더인지 확인하세요."
        case .noSnapshot?:
            return noSnapshot
        case .helper?, nil:
            break
        }
        if text.contains("No space left on device") || text.contains("bytes free to stage")
            || (error.domain == NSCocoaErrorDomain && error.code == NSFileWriteOutOfSpaceError) {
            return "Mac 저장 공간이 부족해 \(failed) 공간을 확보한 뒤 다시 시도하세요."
        }
        if text.contains("does not match the hash") || text.contains("unusable snapshot manifest") {
            return operation == .create
                ? "새 스냅샷이 검증을 통과하지 못했으니 스냅샷을 다시 만드세요."
                : "스냅샷이 손상되어 되돌리지 못했으니 새 스냅샷을 만드세요."
        }
        return "\(failed) 다시 시도하고, 계속 실패하면 아래 이벤트 피드에서 자세한 내용을 확인하세요."
    }
}
