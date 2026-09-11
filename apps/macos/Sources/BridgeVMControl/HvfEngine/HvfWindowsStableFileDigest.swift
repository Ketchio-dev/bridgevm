import CryptoKit
import Darwin
import Foundation

enum HvfWindowsStableFileDigest {
    static func compute(
        _ path: String,
        readChunk: (FileHandle) throws -> Data = { try $0.read(upToCount: 8 * 1024 * 1024) ?? Data() }
    ) throws -> String {
        let descriptor = open(path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        guard descriptor >= 0 else { throw CocoaError(.fileReadUnknown) }
        defer { close(descriptor) }
        var before = stat()
        guard fstat(descriptor, &before) == 0, before.st_mode & S_IFMT == S_IFREG,
              before.st_size >= 0 else { throw CocoaError(.fileReadCorruptFile) }
        let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: false)
        var hasher = SHA256()
        var bytes: Int64 = 0
        while true {
            let chunk = try readChunk(handle)
            if chunk.isEmpty { break }
            let (total, overflow) = bytes.addingReportingOverflow(Int64(chunk.count))
            guard !overflow, total <= before.st_size else { throw CocoaError(.fileReadCorruptFile) }
            bytes = total
            hasher.update(data: chunk)
        }
        var after = stat()
        var named = stat()
        guard bytes == before.st_size, fstat(descriptor, &after) == 0,
              lstat(path, &named) == 0, unchanged(before, after),
              unchanged(after, named) else { throw CocoaError(.fileReadCorruptFile) }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }

    private static func unchanged(_ lhs: stat, _ rhs: stat) -> Bool {
        lhs.st_dev == rhs.st_dev && lhs.st_ino == rhs.st_ino &&
        lhs.st_mode == rhs.st_mode && lhs.st_size == rhs.st_size &&
        lhs.st_mtimespec.tv_sec == rhs.st_mtimespec.tv_sec &&
        lhs.st_mtimespec.tv_nsec == rhs.st_mtimespec.tv_nsec &&
        lhs.st_ctimespec.tv_sec == rhs.st_ctimespec.tv_sec &&
        lhs.st_ctimespec.tv_nsec == rhs.st_ctimespec.tv_nsec
    }
}
