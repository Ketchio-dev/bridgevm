import Darwin
import Foundation

/// A timed-out handler retains its admission slot until it acknowledges completion.
/// The descriptor is closed by its I/O worker only, so shutdown cannot cause fd reuse.
final class NativeRuntimeTransportConnection: @unchecked Sendable {
    let descriptor: Int32
    let deadline: TimeInterval
    private let mutex = NSLock()
    private let ready = DispatchSemaphore(value: 0)
    private var result: Result<NativeRuntimeResponse, Error>?
    private var task: Task<Void, Never>?
    private var stopped = false
    private var closed = false

    init(descriptor: Int32) {
        self.descriptor = descriptor
        deadline = NativeRuntimeTransport.now + NativeRuntimeTransport.timeout
    }

    func begin(request: NativeRuntimeRequest, handler: @escaping NativeRuntimeServer.Handler,
               finished: @escaping @Sendable () -> Void) {
        mutex.lock(); defer { mutex.unlock() }
        guard !stopped else { ready.signal(); finished(); return }
        task = Task.detached { [self] in
            let value: Result<NativeRuntimeResponse, Error>
            do {
                try Task.checkCancellation()
                try NativeRuntimeTransport.checkDeadline(deadline)
                value = .success(try await handler(request))
            } catch { value = .failure(error) }
            complete(value)
            finished()
        }
    }

    private func complete(_ value: Result<NativeRuntimeResponse, Error>) {
        mutex.lock()
        if !stopped { result = value }
        task = nil
        mutex.unlock()
        ready.signal()
    }

    func response() throws -> NativeRuntimeResponse {
        let remaining = deadline - NativeRuntimeTransport.now
        guard remaining > 0, ready.wait(timeout: .now() + remaining) == .success,
              NativeRuntimeTransport.now < deadline else { throw NativeRuntimeError.timedOut }
        mutex.lock(); defer { mutex.unlock() }
        guard !stopped, let result else { throw NativeRuntimeError.transportFailure }
        return try result.get()
    }

    func stop() {
        mutex.lock(); defer { mutex.unlock() }
        stopped = true
        task?.cancel()
        if !closed { _ = shutdown(descriptor, SHUT_RDWR) }
        ready.signal()
    }

    func closeIO() {
        mutex.lock(); defer { mutex.unlock() }
        stopped = true
        task?.cancel()
        if !closed { Darwin.close(descriptor); closed = true }
    }

    deinit { closeIO() }
}
