import Foundation

/// Records an actual failed exclusive publication, independently of whether the
/// competing registration is still present when the caller handles the result.
struct VMRegistrationCollision: LocalizedError {
    let vmID: String

    var errorDescription: String? {
        "Another registration already used the VM ID “\(vmID)”. BridgeVM kept the prepared files. Check your library before trying again."
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
