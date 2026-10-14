import Darwin
import Foundation

enum LibraryWarningPhysicalPath {
    // Foundation's URL resolver can retain /var while directory enumeration uses /private/var.
    static func of(_ file: URL) throws -> String {
        guard let path = realpath(file.path, nil) else {
            throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno))
        }
        defer { free(path) }
        return String(cString: path)
    }
}
