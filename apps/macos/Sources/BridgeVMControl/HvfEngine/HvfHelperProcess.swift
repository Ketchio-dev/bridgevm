import Foundation

/// Runs a finite, non-interactive helper with stdout and stderr on one pipe.
/// The pipe is drained to EOF before the wait, so the helper cannot block on a
/// full pipe. Output past the limit is not retained and is never a success.
enum HvfHelperProcess {
    static let outputLimit = 1 << 20
    static let overflowTail = 4 * 1024

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

    static func run(_ executable: URL, _ arguments: [String]) throws -> Completion {
        let process = Process()
        let pipe = Pipe()
        let reader = pipe.fileHandleForReading
        process.executableURL = executable
        process.arguments = arguments
        process.environment = ["PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LANG": "C"]
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = pipe
        process.standardError = pipe
        try process.run()
        // Only the child may hold the write end, or EOF never arrives.
        try? pipe.fileHandleForWriting.close()
        var retained = Data()
        var total: UInt64 = 0
        var readFailure: Error?
        do {
            // Keep draining past the limit: a child blocked on a full pipe never exits.
            while let chunk = try reader.read(upToCount: 64 * 1024), !chunk.isEmpty {
                total += UInt64(chunk.count)
                retained.append(chunk)
                let keep = total > UInt64(outputLimit) ? overflowTail : outputLimit
                if retained.count > keep { retained = Data(retained.suffix(keep)) }
            }
        } catch {
            readFailure = error
        }
        // After a read failure, closing makes further child writes fail so the wait ends.
        try? reader.close()
        process.waitUntilExit()
        if let readFailure { throw readFailure }
        return Completion(output: retained, totalBytes: total,
                          status: process.terminationStatus, reason: process.terminationReason)
    }
}
