import Foundation

extension VMConfig {
    /// The saved absolute bundle path is definitively gone (lstat ENOENT, e.g. a
    /// detached volume). A relative path is never absent; other lookup errors throw.
    func bundleIsAbsent() throws -> Bool {
        try bundlePath.hasPrefix("/") && HvfUnavailableMediaDeletion.absent(bundlePath)
    }

    /// Absent bundle and nothing managed left in the entry: a stale saved path whose
    /// real bundle (and install request) still lives in the entry is not detached.
    func isDetachedRegistration(rootURL: URL) throws -> Bool {
        try bundleIsAbsent() && VMLibrary.deletionImpact(for: self, rootURL: rootURL) == .registrationOnly
    }
}
