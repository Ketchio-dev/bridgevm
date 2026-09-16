import Foundation

enum HvfOwnedRuntimeChannelEvent: Sendable {
    case message(HvfOwnedRuntimeEvent)
    case eof
    case failed(HvfOwnedRuntimeProtocolError)
}

/// Pipes have one worker owner. No descriptor close races with an in-flight read or write.
final class HvfOwnedRuntimeChannel: @unchecked Sendable {
    let input = Pipe()
    let output = Pipe()
    let events: AsyncStream<HvfOwnedRuntimeChannelEvent>
    private let continuation: AsyncStream<HvfOwnedRuntimeChannelEvent>.Continuation
    private let queue = DispatchQueue(label: "com.bridgevm.owned-runtime-channel")
    private let lock = NSLock()
    private var closed = false
    private var started = false
    private var stopFrame: Data?
    private var stopQueued = false

    init() throws {
        let pair = AsyncStream<HvfOwnedRuntimeChannelEvent>.makeStream(bufferingPolicy: .bufferingOldest(32))
        events = pair.stream; continuation = pair.continuation
        try HvfOwnedRuntimePipeIO.configure(input.fileHandleForWriting, writing: true)
        try HvfOwnedRuntimePipeIO.configure(output.fileHandleForReading, writing: false)
    }

    func attach(to process: Process) { process.standardInput = input; process.standardOutput = output }

    func start(helloFrame: Data) {
        lock.lock(); precondition(!started); started = true; lock.unlock()
        try? input.fileHandleForReading.close()
        try? output.fileHandleForWriting.close()
        let initial = PendingHello(helloFrame)
        queue.async { [self] in run(initial: initial) }
    }

    func sendStop(_ message: HvfOwnedRuntimeStop) throws {
        let frame = try HvfOwnedRuntimeCodec.frame(message)
        lock.lock(); defer { lock.unlock() }
        guard !closed else { throw HvfOwnedRuntimeProtocolError.channelClosed }
        guard !stopQueued else { return }
        stopQueued = true; stopFrame = frame
    }

    func close() {
        lock.lock(); closed = true; let hasWorker = started; lock.unlock()
        if !hasWorker { closeHandles(); continuation.finish() }
    }

    private func closeHandles() {
        try? input.fileHandleForReading.close(); try? input.fileHandleForWriting.close()
        try? output.fileHandleForReading.close(); try? output.fileHandleForWriting.close()
    }

    private func takePending() -> (Bool, Data?) {
        lock.lock(); defer { lock.unlock() }
        let pending = stopFrame; stopFrame = nil
        return (closed, pending)
    }

    private func emit(_ event: HvfOwnedRuntimeChannelEvent) throws {
        switch continuation.yield(event) {
        case .enqueued: return
        case .dropped, .terminated: throw HvfOwnedRuntimeProtocolError.channelClosed
        @unknown default: throw HvfOwnedRuntimeProtocolError.channelClosed
        }
    }

    private final class PendingHello: @unchecked Sendable {
        private var frame: Data
        init(_ frame: Data) { self.frame = frame }
        func take() -> Data { var value = Data(); swap(&value, &frame); return value }
        deinit { frame.resetBytes(in: frame.indices) }
    }

    private func run(initial: PendingHello) {
        var writing = initial.take()
        var offset = 0
        var writeDeadline = ProcessInfo.processInfo.systemUptime + 2
        var reading = Data()
        var partialDeadline: TimeInterval?
        var queued: Data?
        var helloWritten = false
        var inputEndedDeadline: TimeInterval?
        defer {
            writing.resetBytes(in: writing.indices)
            closeHandles(); continuation.finish()
            lock.lock(); closed = true; stopFrame = nil; lock.unlock()
        }
        do {
            while true {
                let (closing, pending) = takePending()
                if closing { return }
                if let pending, inputEndedDeadline == nil { queued = pending }
                let now = ProcessInfo.processInfo.systemUptime
                if offset == writing.count, let pending = queued {
                    writing.resetBytes(in: writing.indices)
                    writing = pending; offset = 0; queued = nil; writeDeadline = now + 2
                }
                if offset < writing.count {
                    guard now < writeDeadline else { throw HvfOwnedRuntimeProtocolError.timedOut }
                    do {
                        try HvfOwnedRuntimePipeIO.write(writing, offset: &offset, to: input.fileHandleForWriting)
                        guard ProcessInfo.processInfo.systemUptime < writeDeadline else { throw HvfOwnedRuntimeProtocolError.timedOut }
                        if offset == writing.count { helloWritten = true; writing.resetBytes(in: writing.indices) }
                    } catch HvfOwnedRuntimeProtocolError.writeClosed where helloWritten {
                        // A natural terminal event can already be buffered when STOP races stdin closure.
                        inputEndedDeadline = ProcessInfo.processInfo.systemUptime + 2
                        offset = writing.count; writing.resetBytes(in: writing.indices); queued = nil
                    }
                }
                let beforeRead = ProcessInfo.processInfo.systemUptime
                if let deadline = partialDeadline, beforeRead >= deadline { throw HvfOwnedRuntimeProtocolError.timedOut }
                if let deadline = inputEndedDeadline, beforeRead >= deadline { throw HvfOwnedRuntimeProtocolError.timedOut }
                if let chunk = try HvfOwnedRuntimePipeIO.read(output.fileHandleForReading) {
                    let afterRead = ProcessInfo.processInfo.systemUptime
                    if let deadline = partialDeadline, afterRead >= deadline { throw HvfOwnedRuntimeProtocolError.timedOut }
                    if let deadline = inputEndedDeadline, afterRead >= deadline { throw HvfOwnedRuntimeProtocolError.timedOut }
                    if chunk.isEmpty {
                        guard reading.isEmpty else { throw HvfOwnedRuntimeProtocolError.invalidFrame }
                        try emit(.eof); return
                    }
                    if reading.isEmpty { partialDeadline = afterRead + 2 }
                    reading.append(chunk)
                    while let body = try HvfOwnedRuntimeCodec.takeFrame(from: &reading) {
                        let message = try HvfOwnedRuntimeCodec.decode(HvfOwnedRuntimeEvent.self, payload: body)
                        if ProcessInfo.processInfo.systemUptime >= min(partialDeadline ?? .infinity, inputEndedDeadline ?? .infinity) {
                            throw HvfOwnedRuntimeProtocolError.timedOut
                        }
                        try emit(.message(message))
                        partialDeadline = reading.isEmpty ? nil : afterRead + 2
                    }
                    guard reading.count <= HvfOwnedRuntimeCodec.maximumPayload + 4 else {
                        throw HvfOwnedRuntimeProtocolError.invalidFrame
                    }
                }
                try HvfOwnedRuntimePipeIO.wait(read: output.fileHandleForReading,
                    write: input.fileHandleForWriting, wantsWrite: offset < writing.count)
            }
        } catch let error as HvfOwnedRuntimeProtocolError {
            _ = continuation.yield(.failed(error))
        } catch { _ = continuation.yield(.failed(.ioFailure)) }
    }

    deinit { close() }
}
