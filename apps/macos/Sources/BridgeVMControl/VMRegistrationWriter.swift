import Foundation

enum VMRegistrationWriter {
    /// Transactional callers use `commit` to distinguish publication from sync.
    static func write(_ data: Data, to url: URL) throws {
        switch commit(data, to: url) {
        case .committed: return
        case .notPublished(let error), .publishedButUnsynced(let error): throw error
        }
    }
}
