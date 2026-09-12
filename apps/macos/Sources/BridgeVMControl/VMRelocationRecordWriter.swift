import Foundation
import Darwin

enum VMRelocationRecordWriter {
    static func write(_ data: Data, to url: URL) throws {
        try VTPMStateSecurity.createPrivateFile(data, at: url)
        let parent = url.deletingLastPathComponent()
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
