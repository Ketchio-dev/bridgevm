import Darwin
import Foundation

/// Appends one already-framed control command. Callers retain creation and service admission.
enum HvfControlFileAppend {
    typealias Write = (Int32, UnsafeRawPointer?, Int) -> Int

    /// One checked write, without retrying a partial command. The dependency is the final write only.
    static func append(_ data: Data, to url: URL, write: Write = Darwin.write) throws {
        // EOF belongs to the write operation: another writer may append after this open.
        let descriptor = Darwin.open(url.path, O_WRONLY | O_APPEND | O_CLOEXEC)
        guard descriptor >= 0 else { throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
        defer { Darwin.close(descriptor) }
        let written = data.withUnsafeBytes { write(descriptor, $0.baseAddress, $0.count) }
        guard written == data.count else {
            throw NSError(domain: "BridgeVM.ControlFileAppend", code: 1,
                          userInfo: [NSLocalizedDescriptionKey: "control command write was incomplete"])
        }
    }
}
