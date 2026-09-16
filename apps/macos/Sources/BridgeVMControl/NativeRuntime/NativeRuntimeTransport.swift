import Darwin
import Foundation

enum NativeRuntimeTransport {
    static let timeout: TimeInterval = 2
    static var now: TimeInterval { ProcessInfo.processInfo.systemUptime }

    static func checkDeadline(_ deadline: TimeInterval) throws {
        guard now < deadline else { throw NativeRuntimeError.timedOut }
    }

    static func socket() throws -> Int32 {
        let fd = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw NativeRuntimeError.transportFailure }
        do { try configure(fd); return fd } catch { Darwin.close(fd); throw error }
    }

    static func configure(_ fd: Int32) throws {
        var noSignal: Int32 = 1
        guard fcntl(fd, F_SETFD, FD_CLOEXEC) == 0,
              fcntl(fd, F_SETFL, O_NONBLOCK) == 0,
              setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &noSignal, socklen_t(MemoryLayout<Int32>.size)) == 0
        else { throw NativeRuntimeError.transportFailure }
    }

    static func withAddress<T>(_ path: String, _ body: (UnsafePointer<sockaddr>, socklen_t) throws -> T) throws -> T {
        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        let bytes = Array(path.utf8) + [0]
        guard !path.utf8.contains(0), bytes.count <= MemoryLayout.size(ofValue: address.sun_path) else {
            throw NativeRuntimeError.invalidEndpoint
        }
        address.sun_len = UInt8(MemoryLayout<sockaddr_un>.size)
        withUnsafeMutableBytes(of: &address.sun_path) { target in target.copyBytes(from: bytes) }
        return try withUnsafePointer(to: &address) {
            try $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                try body($0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
    }

    static func verifyPeer(_ fd: Int32, expectedUID: uid_t = geteuid()) throws {
        var uid: uid_t = 0, gid: gid_t = 0
        guard getpeereid(fd, &uid, &gid) == 0, uid == expectedUID else { throw NativeRuntimeError.peerRejected }
    }

    static func wait(_ fd: Int32, events: Int16, deadline: TimeInterval) throws {
        while true {
            let remaining = deadline - now
            guard remaining > 0 else { throw NativeRuntimeError.timedOut }
            var item = pollfd(fd: fd, events: events, revents: 0)
            let result = poll(&item, 1, Int32(min(remaining * 1000 + 1, Double(Int32.max))))
            if result < 0, errno == EINTR { continue }
            guard result >= 0 else { throw NativeRuntimeError.transportFailure }
            if result == 0 { continue }
            try checkDeadline(deadline)
            guard item.revents & Int16(POLLNVAL) == 0 else { throw NativeRuntimeError.transportFailure }
            return
        }
    }

    static func connect(_ fd: Int32, path: String, deadline: TimeInterval) throws {
        let result = try withAddress(path) { Darwin.connect(fd, $0, $1) }
        if result != 0 {
            guard errno == EINPROGRESS || errno == EAGAIN else { throw NativeRuntimeError.ownerUnavailable }
            try wait(fd, events: Int16(POLLOUT), deadline: deadline)
            var error: Int32 = 0, size = socklen_t(MemoryLayout<Int32>.size)
            guard getsockopt(fd, SOL_SOCKET, SO_ERROR, &error, &size) == 0, error == 0 else {
                throw NativeRuntimeError.ownerUnavailable
            }
        }
        try checkDeadline(deadline)
    }

    static func readFrame(_ fd: Int32, limit: Int, deadline: TimeInterval) throws -> Data {
        let header = try read(fd, count: 4, deadline: deadline)
        let count = header.reduce(0) { ($0 << 8) | Int($1) }
        guard count > 0, count <= limit else { throw NativeRuntimeError.invalidMessage }
        return try read(fd, count: count, deadline: deadline)
    }

    static func writeFrame(_ data: Data, to fd: Int32, limit: Int, deadline: TimeInterval) throws {
        guard !data.isEmpty, data.count <= limit else { throw NativeRuntimeError.invalidMessage }
        let count = UInt32(data.count)
        let header = Data([UInt8(count >> 24), UInt8((count >> 16) & 255), UInt8((count >> 8) & 255), UInt8(count & 255)])
        try write(header + data, to: fd, deadline: deadline)
    }

    private static func read(_ fd: Int32, count: Int, deadline: TimeInterval) throws -> Data {
        var result = Data(), buffer = [UInt8](repeating: 0, count: min(count, 8192))
        while result.count < count {
            try wait(fd, events: Int16(POLLIN), deadline: deadline)
            let received = Darwin.read(fd, &buffer, min(buffer.count, count - result.count))
            if received < 0, errno == EINTR || errno == EAGAIN { continue }
            guard received > 0 else { throw NativeRuntimeError.transportFailure }
            try checkDeadline(deadline)
            result.append(contentsOf: buffer.prefix(received))
        }
        return result
    }

    private static func write(_ data: Data, to fd: Int32, deadline: TimeInterval) throws {
        try data.withUnsafeBytes { buffer in
            var sent = 0
            while sent < buffer.count {
                try wait(fd, events: Int16(POLLOUT), deadline: deadline)
                let count = Darwin.write(fd, buffer.baseAddress!.advanced(by: sent), buffer.count - sent)
                if count < 0, errno == EINTR || errno == EAGAIN { continue }
                guard count > 0 else { throw NativeRuntimeError.transportFailure }
                try checkDeadline(deadline)
                sent += count
            }
        }
    }
}
