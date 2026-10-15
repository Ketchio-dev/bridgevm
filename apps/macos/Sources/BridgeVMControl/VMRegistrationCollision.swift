import Foundation

/// Records an actual failed exclusive publication, independently of whether the
/// competing registration is still present when the caller handles the result.
struct VMRegistrationCollision: LocalizedError {
    var errorDescription: String? {
        "같은 VM ID의 등록 정보가 이미 있습니다. 준비된 파일은 보존했습니다. 라이브러리를 확인하세요."
    }
}

extension VMRegistrationCommitOutcome {
    var preservesPreparedDestination: Bool {
        switch self {
        case .committed, .publishedButUnsynced: return true
        case .notPublished(let error): return error is VMRegistrationCollision
        }
    }
}
