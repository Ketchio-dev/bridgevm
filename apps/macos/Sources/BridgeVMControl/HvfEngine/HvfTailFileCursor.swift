import Foundation
#if canImport(Darwin)
import Darwin
#else
import Glibc
#endif

/// Identity and length come from the same descriptor that supplies the bytes.
struct HvfTailFileCursor {
    private struct Identity: Equatable {
        let path: String
        let device: UInt64
        let inode: UInt64
    }
    private var identity: Identity?
    private(set) var offset: UInt64
    init(startingAt offset: UInt64) { self.offset = offset }

    static func openFile(_ url: URL) -> FileHandle? {
        let descriptor = url.withUnsafeFileSystemRepresentation { path in
            path.map { open($0, O_RDONLY | O_NONBLOCK | O_CLOEXEC) } ?? -1
        }
        guard descriptor >= 0 else { return nil }
        return FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
    }

    mutating func admit(_ handle: FileHandle, path: String) -> (size: UInt64, reset: Bool)? {
        var state = stat()
        guard fstat(handle.fileDescriptor, &state) == 0, state.st_size >= 0,
              state.st_mode & mode_t(S_IFMT) == mode_t(S_IFREG) else { return nil }
        let next = Identity(path: path, device: UInt64(truncatingIfNeeded: state.st_dev), inode: UInt64(truncatingIfNeeded: state.st_ino))
        let size = UInt64(state.st_size)
        let reset = (identity != nil && identity != next) || size < offset
        if reset { offset = 0 }
        identity = next
        return (size, reset)
    }

    mutating func advance(_ bytes: Int) { offset += UInt64(bytes) }
}
