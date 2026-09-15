import Foundation
import Darwin

enum VMRegistrationCommitOutcome {
    case committed
    case notPublished(Error)
    case publishedButUnsynced(Error)

    var isCommitted: Bool {
        if case .committed = self { return true }
        return false
    }
}

extension VMRegistrationWriter {
    static func commit(
        _ data: Data, to url: URL,
        syncParent: (URL) throws -> Void = syncParentDirectory
    ) -> VMRegistrationCommitOutcome {
        let parent = url.deletingLastPathComponent()
        let temporary = parent.appendingPathComponent(".vm-config-\(UUID().uuidString).tmp")
        var published = false
        do {
            try VTPMStateSecurity.createPrivateFile(data, at: temporary)
            defer { try? FileManager.default.removeItem(at: temporary) }
            guard Darwin.rename(temporary.path, url.path) == 0 else {
                throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno))
            }
            published = true
            try syncParent(parent)
            return .committed
        } catch {
            return published ? .publishedButUnsynced(error) : .notPublished(error)
        }
    }

    private static func syncParentDirectory(_ parent: URL) throws {
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
