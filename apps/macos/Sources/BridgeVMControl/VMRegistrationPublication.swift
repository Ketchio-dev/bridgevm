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
        replaceExisting: Bool = true,
        syncParent: (URL) throws -> Void = syncParentDirectory
    ) -> VMRegistrationCommitOutcome {
        let parent = url.deletingLastPathComponent()
        let temporary = parent.appendingPathComponent(".vm-config-\(UUID().uuidString).tmp")
        var published = false
        do {
            try VTPMStateSecurity.createPrivateFile(data, at: temporary)
            defer { try? FileManager.default.removeItem(at: temporary) }
            let renamed = replaceExisting
                ? Darwin.rename(temporary.path, url.path)
                : Darwin.renameatx_np(AT_FDCWD, temporary.path, AT_FDCWD, url.path, UInt32(RENAME_EXCL))
            guard renamed == 0 else {
                if !replaceExisting, errno == EEXIST { throw VMRegistrationCollision() }
                throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno))
            }
            published = true
            try syncParent(parent)
            return .committed
        } catch {
            return published ? .publishedButUnsynced(error) : .notPublished(error)
        }
    }

}
