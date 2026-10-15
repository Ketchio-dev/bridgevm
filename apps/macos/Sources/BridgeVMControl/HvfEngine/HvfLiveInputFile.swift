import Darwin
import Foundation

/// One legacy channel identity until an explicit session lifecycle reset.
/// The consumer may compact this inode, but a replacement is not the same channel.
final class HvfLiveInputFile {
    private var handle: FileHandle?
    private var path: URL?
    private var identity: stat?

    func append(_ data: Data, to url: URL) throws {
        guard path == nil || path == url else { throw failure() }
        if handle == nil { try open(url) }
        guard let handle, let identity else { throw failure() }
        let descriptor = handle.fileDescriptor
        guard flock(descriptor, LOCK_EX) == 0 else { throw failure() }
        defer { flock(descriptor, LOCK_UN) }
        var opened = stat(), named = stat()
        guard fstat(descriptor, &opened) == 0, lstat(url.path, &named) == 0,
              Self.regularOwned(opened), Self.regularOwned(named),
              Self.same(identity, opened), Self.same(identity, named) else { throw failure() }
        // A cached offset is stale after the consumer truncates this same inode.
        try handle.seekToEnd()
        try handle.write(contentsOf: data)
    }

    private func open(_ url: URL) throws {
        let flags = O_WRONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC
        var descriptor = Darwin.open(url.path, flags)
        if descriptor < 0, errno == ENOENT {
            descriptor = Darwin.open(url.path, flags | O_CREAT | O_EXCL, 0o600)
        }
        guard descriptor >= 0 else { throw failure() }
        var opened = stat(), named = stat()
        guard fstat(descriptor, &opened) == 0, lstat(url.path, &named) == 0,
              Self.regularOwned(opened), Self.regularOwned(named), Self.same(opened, named) else {
            Darwin.close(descriptor)
            throw failure()
        }
        handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        identity = opened
        path = url
    }

    func close() {
        try? handle?.close()
        handle = nil
        identity = nil
        path = nil
    }

    private static func same(_ lhs: stat, _ rhs: stat) -> Bool {
        lhs.st_dev == rhs.st_dev && lhs.st_ino == rhs.st_ino
    }

    private static func regularOwned(_ value: stat) -> Bool {
        value.st_mode & S_IFMT == S_IFREG && value.st_uid == geteuid() && value.st_nlink == 1
    }

    private func failure() -> NSError {
        NSError(domain: "BridgeVM.LiveInputFile", code: 1,
                userInfo: [NSLocalizedDescriptionKey: "input channel is unavailable, unsafe or replaced"])
    }

    deinit { close() }
}
