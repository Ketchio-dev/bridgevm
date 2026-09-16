import Foundation
import Darwin

/// Each descriptor is used and closed on the channel's worker queue after launch.
enum HvfOwnedRuntimePipeIO {
    static func configure(_ handle: FileHandle, writing: Bool) throws {
        let fd = handle.fileDescriptor
        let flags = fcntl(fd, F_GETFL)
        guard flags >= 0, fcntl(fd, F_SETFL, flags | O_NONBLOCK) == 0,
              fcntl(fd, F_SETFD, FD_CLOEXEC) == 0 else { throw HvfOwnedRuntimeProtocolError.ioFailure }
        if writing, fcntl(fd, F_SETNOSIGPIPE, 1) != 0 { throw HvfOwnedRuntimeProtocolError.ioFailure }
    }

    static func read(_ handle: FileHandle) throws -> Data? {
        var bytes = [UInt8](repeating: 0, count: 4096)
        let count = Darwin.read(handle.fileDescriptor, &bytes, bytes.count)
        if count > 0 { return Data(bytes.prefix(count)) }
        if count == 0 { return Data() }
        if errno == EAGAIN || errno == EINTR { return nil }
        throw HvfOwnedRuntimeProtocolError.ioFailure
    }

    static func write(_ data: Data, offset: inout Int, to handle: FileHandle) throws {
        let count = data.withUnsafeBytes { raw -> Int in
            guard let address = raw.baseAddress else { return 0 }
            return Darwin.write(handle.fileDescriptor, address.advanced(by: offset), data.count - offset)
        }
        if count > 0 { offset += count; return }
        if count < 0, errno == EAGAIN || errno == EINTR { return }
        if count < 0, errno == EPIPE { throw HvfOwnedRuntimeProtocolError.writeClosed }
        throw HvfOwnedRuntimeProtocolError.ioFailure
    }

    static func wait(read: FileHandle, write: FileHandle, wantsWrite: Bool) throws {
        var descriptors = [pollfd(fd: read.fileDescriptor, events: Int16(POLLIN), revents: 0),
                           pollfd(fd: wantsWrite ? write.fileDescriptor : -1, events: Int16(POLLOUT), revents: 0)]
        let result = Darwin.poll(&descriptors, nfds_t(descriptors.count), 20)
        if result < 0, errno != EINTR { throw HvfOwnedRuntimeProtocolError.ioFailure }
    }
}
