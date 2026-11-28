import Foundation

/// Records an actual failed exclusive publication, independently of whether the
/// competing registration is still present when the caller handles the result.
struct VMRegistrationCollision: LocalizedError {
    let vmID: String

    var errorDescription: String? {
        "Another VM with the same ID was added to your library at the same time, so BridgeVM kept the files it prepared. Check your library, then try again."
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
