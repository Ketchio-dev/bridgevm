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

    /// The helper was stopped by a forwarded SIGINT or SIGTERM and did not succeed.
    struct Cancelled: Error { let signal: Int32 }

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
        let forwarder = forwardsCancelSignals ? SignalForwarder() : nil
        defer { _ = forwarder?.finish() }
        try process.run()
        forwarder?.forward(to: process.processIdentifier)
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
        let cancelled = forwarder?.finish()
        if let readFailure { throw readFailure }
        let completion = Completion(output: retained, totalBytes: total,
                                    status: process.terminationStatus, reason: process.terminationReason)
        // A helper that finished before the signal reached it completed its work.
        if let cancelled, !completion.succeeded { throw Cancelled(signal: cancelled) }
        return completion
    }

    /// For one helper run, takes SIGINT and SIGTERM from their default action and sends the helper
    /// SIGTERM instead. A signal the CLI inherited as ignored (a background job) stays ignored.
    /// Foundation resets the child's signal dispositions, so the helper still dies of SIGTERM.
    private final class SignalForwarder {
        private let queue = DispatchQueue(label: "BridgeVM.HelperSignalForwarder")
        private var previous: [(Int32, sig_t?)] = []
        private var sources: [DispatchSourceSignal] = []
        private var helper: pid_t = 0
        private var received: Int32?

        init() {
            for number in [SIGINT, SIGTERM] {
                let old = signal(number, SIG_IGN)
                previous.append((number, old))
                if let old, unsafeBitCast(old, to: Int.self) == unsafeBitCast(SIG_IGN, to: Int.self) { continue }
                let source = DispatchSource.makeSignalSource(signal: number, queue: queue)
                // Handlers run on `queue`, and finish cancels them there, so none outlives self.
                source.setEventHandler { [unowned self] in
                    if received == nil { received = number }
                    if helper > 0 { kill(helper, SIGTERM) }
                }
                source.resume()
                sources.append(source)
            }
        }

        /// A signal that arrived while the helper was being started is forwarded now.
        func forward(to pid: pid_t) {
            queue.sync {
                helper = pid
                if received != nil { kill(pid, SIGTERM) }
            }
        }

        /// Called once the helper has exited, before its PID can be reused. Returns the signal
        /// that asked to cancel it, if any; a repeated call only returns that signal.
        func finish() -> Int32? {
            let signalled = queue.sync {
                helper = 0
                for source in sources { source.cancel() }
                sources = []
                return received
            }
            for (number, old) in previous { signal(number, old) }
            previous = []
            return signalled
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
