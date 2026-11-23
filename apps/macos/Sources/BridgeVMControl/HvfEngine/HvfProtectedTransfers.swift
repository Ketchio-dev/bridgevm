import Foundation

/// UI entry points only. A failed session does not prove the operation rolled
/// back; callers must not report restoration of the original location.
enum HvfProtectedTransfers {
    static func clone(name: String, template: VMConfig, libraryRoot: URL) -> VMConfig? {
        HvfMediaLeaseSession.withCopyOwnership(config: template) { afterCopy in
            VMLibrary.cloneWindowsHVF(name: name, template: template, libraryRoot: libraryRoot, afterCopy: afterCopy)
        }
    }

    static func move(_ config: VMConfig, to destination: URL, rootURL: URL) -> VMConfig? {
        try? HvfMediaLeaseSession.withOwnership(config: config) {
            VMLibrary.moveWindowsHVFBundle(config, to: destination, rootURL: rootURL)
        }
    }
}

extension HvfProtectedTransfers {
    static func delete(_ config: VMConfig, rootURL: URL) -> Bool {
        if let unavailable = HvfUnavailableMediaDeletion.delete(config, rootURL: rootURL) { return unavailable }
        return (try? HvfDeletionOwnership.withOwnership(config: config, rootURL: rootURL) {
            LibraryDeletionReferences.delete(config, rootURL: rootURL) }) == true
    }
}
