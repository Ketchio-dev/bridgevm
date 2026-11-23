import Foundation
import Darwin

enum VMRegistrationWriter {
    /// Transactional callers use `commit` to distinguish publication from sync.
    static func write(_ data: Data, to url: URL) throws {
        switch commit(data, to: url) {
        case .committed: return
        case .notPublished(let error), .publishedButUnsynced(let error): throw error
        }
    }
}

extension VMRegistrationWriter {
    static func syncParentDirectory(_ parent: URL) throws {
        let descriptor = Darwin.open(parent.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard descriptor >= 0 else {
            throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno))
        }
        defer { Darwin.close(descriptor) }
        guard Darwin.fsync(descriptor) == 0 else {
            throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno))
        }
    }
}
