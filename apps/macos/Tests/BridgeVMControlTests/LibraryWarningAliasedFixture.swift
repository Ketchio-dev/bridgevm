import Foundation

/// Exercise canonical issue paths even when the host temporary root has no alias.
struct LibraryWarningAliasedFixture {
    let owned: URL
    let library: URL

    init() throws {
        let base = ProcessInfo.processInfo.environment["TMPDIR"] ?? NSTemporaryDirectory()
        owned = URL(fileURLWithPath: base).appendingPathComponent("library-warning-" + UUID().uuidString)
        let actual = owned.appendingPathComponent("actual")
        let alias = owned.appendingPathComponent("alias")
        library = alias.appendingPathComponent("library")
        do {
            try FileManager.default.createDirectory(at: actual.appendingPathComponent("library"), withIntermediateDirectories: true)
            try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: actual)
        } catch {
            try? FileManager.default.removeItem(at: owned)
            throw error
        }
    }

    func remove() { try? FileManager.default.removeItem(at: owned) }
}
