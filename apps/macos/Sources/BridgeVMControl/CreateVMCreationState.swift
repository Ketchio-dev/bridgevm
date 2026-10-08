import Foundation

/// A published registration cannot be rolled back or retried as a fresh creation.
struct CreateVMCreationState {
    private(set) var published: VMConfig?
    private(set) var publicationWarning = ""
    var recoveryFailureMessage: String {
        [publicationWarning, "저장된 VM을 불러오지 못했습니다. 파일은 보존했습니다. 다시 만들지 말고 저장된 VM을 다시 불러오세요."].filter { !$0.isEmpty }.joined(separator: "\n")
    }
    var permitsCreation: Bool { published == nil }

    mutating func complete(
        _ outcome: HVFCreationOutcome?, adopt: (VMConfig) -> Bool
    ) -> (dismiss: Bool, code: String, message: String) {
        switch outcome {
        case .created(let config):
            if adopt(config) { return (true, "", "") }
            published = config
            return (false, "library-publication-failed", "저장된 VM을 라이브러리에서 찾지 못했습니다. 파일은 보존했습니다. 다시 만들지 말고 저장된 VM을 불러오세요.")
        case .publishedButUnsynced(let config, let message):
            published = config
            publicationWarning = message
            return (false, "vm-publication-unconfirmed", message)
        case nil:
            return (false, "vm-materialization-failed", "생성 또는 VM 라이브러리 저장 실패")
        }
    }

    /// Only read/adopt the saved registration. This does not confirm its prior fsync.
    mutating func recover(adopt: (VMConfig) -> Bool) -> Bool {
        guard let published else { return false }
        guard adopt(published) else { return false }
        self.published = nil
        publicationWarning = ""
        return true
    }
}
