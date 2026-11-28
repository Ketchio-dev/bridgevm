import Darwin
import Foundation

/// Runs a finite, non-interactive helper with stdout and stderr on one pipe.
/// The pipe is drained to EOF before the wait, so the helper cannot block on a
/// full pipe. Output past the limit is not retained and is never a success.
enum HvfHelperProcess {
    static let outputLimit = 1 << 20
    static let overflowTail = 4 * 1024

    /// Set by the native CLI only; the app never sets it. Foundation starts the helper in its own
    /// process group, so Ctrl-C and SIGTERM reach only the CLI and the helper would run on. With
    /// this set, `run` passes them to the helper as SIGTERM, waits for it, and throws `Cancelled`.
    /// Killing the snapshot helper leaves the old pair or the complete new one.
    static var forwardsCancelSignals = false

    /// The helper was stopped by a forwarded SIGINT or SIGTERM, or by its cancel handle, and did not succeed.
    struct Cancelled: Error { let signal: Int32 }

    /// Helper runs in a task bound to a handle (the app's import) stop when the user cancels it.
    @TaskLocal static var cancelHandle: CancelHandle?

    /// `cancel` sends the running helper SIGTERM, and a later run with this handle is stopped as it
    /// starts. A helper stopped this way removes its partial copy.
    final class CancelHandle: @unchecked Sendable {
        private let lock = NSLock()
        private var helper: pid_t = 0
        private var received: Int32?

        var isCancelled: Bool { lock.withLock { received != nil } }

        func cancel(signal: Int32 = SIGTERM) {
            lock.withLock {
                if received == nil { received = signal }
                if helper > 0 { kill(helper, SIGTERM) }
            }
        }

        /// A cancel that arrived while the helper was being started is sent now.
        fileprivate func started(_ pid: pid_t) {
            lock.withLock {
                helper = pid
                if received != nil { kill(pid, SIGTERM) }
            }
        }

        /// Called once the helper has exited, before its PID can be reused; returns the cancel's signal.
        fileprivate func exited() -> Int32? { lock.withLock { helper = 0; return received } }
    }

    struct Completion {
        let output: Data
        let totalBytes: UInt64
        let status: Int32
        let reason: Process.TerminationReason

        var overflowed: Bool { totalBytes > UInt64(HvfHelperProcess.outputLimit) }
        var succeeded: Bool { reason == .exit && status == 0 && !overflowed }
        var text: String { String(decoding: output, as: UTF8.self) }

        /// Trimmed output; after an overflow, a header and the final bytes only.
        var diagnostic: String {
            let body = text.trimmingCharacters(in: .whitespacesAndNewlines)
            guard overflowed else { return body }
            return "helper output exceeded \(HvfHelperProcess.outputLimit) bytes (\(totalBytes) read); "
                + "the operation may have completed, so inspect its state before retrying\n" + body
        }
    }

    /// With `progress`, the helper's `progress <percent>` lines are reported and not retained.
    static func run(_ executable: URL, _ arguments: [String],
                    progress: ((Int) -> Void)? = nil) throws -> Completion {
        let process = Process()
        let pipe = Pipe()
        let reader = pipe.fileHandleForReading
        process.executableURL = executable
        process.arguments = arguments
        process.environment = ["PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LANG": "C"]
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = pipe
        process.standardError = pipe
        let handle = cancelHandle ?? (forwardsCancelSignals ? CancelHandle() : nil)
        let forwarder = forwardsCancelSignals ? handle.map(SignalForwarder.init) : nil
        defer { forwarder?.finish() }
        try process.run()
        handle?.started(process.processIdentifier)
        // Only the child may hold the write end, or EOF never arrives.
        try? pipe.fileHandleForWriting.close()
        var retained = Data()
        var total: UInt64 = 0
        var readFailure: Error?
        var lines = ProgressLines(report: progress)
        func retain(_ chunk: Data) {
            total += UInt64(chunk.count)
            retained.append(chunk)
            let keep = total > UInt64(outputLimit) ? overflowTail : outputLimit
            if retained.count > keep { retained = Data(retained.suffix(keep)) }
        }
        // FileHandle.read(upToCount:) may wait for the whole buffer. A pipe read returns
        // available bytes so a short progress line reaches the UI while conversion is running.
        var buffer = [UInt8](repeating: 0, count: 64 * 1024)
        while true {
            let count = Darwin.read(reader.fileDescriptor, &buffer, buffer.count)
            if count == 0 { break }
            if count < 0 {
                if errno == EINTR { continue }
                readFailure = POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
                break
            }
            retain(lines.filter(Data(buffer.prefix(count))))
        }
        retain(lines.finish())
        // After a read failure, closing makes further child writes fail so the wait ends.
        try? reader.close()
        process.waitUntilExit()
        forwarder?.finish()
        let cancelled = handle?.exited()
        if let readFailure { throw readFailure }
        let completion = Completion(output: retained, totalBytes: total,
                                    status: process.terminationStatus, reason: process.terminationReason)
        // A helper that finished before the signal reached it completed its work.
        if let cancelled, !completion.succeeded { throw Cancelled(signal: cancelled) }
        return completion
    }

    /// For one helper run, takes SIGINT and SIGTERM from their default action and cancels the helper
    /// instead. A signal the CLI inherited as ignored (a background job) stays ignored.
    /// Foundation resets the child's signal dispositions, so the helper still dies of SIGTERM.
    private final class SignalForwarder {
        private let queue = DispatchQueue(label: "BridgeVM.HelperSignalForwarder")
        private var previous: [(Int32, sig_t?)] = []
        private var sources: [DispatchSourceSignal] = []

        init(_ handle: CancelHandle) {
            for number in [SIGINT, SIGTERM] {
                let old = signal(number, SIG_IGN)
                previous.append((number, old))
                if let old, unsafeBitCast(old, to: Int.self) == unsafeBitCast(SIG_IGN, to: Int.self) { continue }
                let source = DispatchSource.makeSignalSource(signal: number, queue: queue)
                source.setEventHandler { handle.cancel(signal: number) }
                source.resume()
                sources.append(source)
            }
        }

        /// Called once the helper has exited; a repeated call does nothing.
        func finish() {
            // Handlers run on `queue`, so none runs after this.
            queue.sync {
                for source in sources { source.cancel() }
                sources = []
            }
            for (number, old) in previous { signal(number, old) }
            previous = []
        }
    }

    /// Takes whole `progress <percent>` lines out of the output; without a reporter all passes through.
    struct ProgressLines {
        let report: ((Int) -> Void)?
        private var pending = Data()

        init(report: ((Int) -> Void)?) { self.report = report }

        mutating func filter(_ chunk: Data) -> Data {
            guard let report else { return chunk }
            pending.append(chunk)
            var kept = Data()
            while let newline = pending.firstIndex(of: 0x0a) {
                let line = pending[pending.startIndex..<newline]
                let text = String(decoding: line, as: UTF8.self)
                if text.hasPrefix("progress "), let percent = Int(text.dropFirst(9)), (0...100).contains(percent) {
                    report(percent)
                } else {
                    kept.append(line)
                    kept.append(0x0a)
                }
                pending = Data(pending[(newline + 1)...])
            }
            // A long unterminated run is not a progress line.
            if pending.count > 4096 { kept.append(pending); pending = Data() }
            return kept
        }

        mutating func finish() -> Data {
            defer { pending = Data() }
            return pending
        }
    }
}
