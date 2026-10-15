import Foundation

/// Only saved registrations in this library are inspected, without recovery.
/// This is a refusal check, not a cross-library or concurrent-writer lock.
enum HvfPendingDeletionReferences {
    static func validate(_ selected: VMConfig, rootURL: URL) throws {
        try LibrarySavedReferences.validate(selected, rootURL: rootURL,
            affectedPath: rootURL.appendingPathComponent(selected.slug))
    }

    /// Also accepts an already-retained request; no saved registrations are loaded here.
    static func references(config: VMConfig, request: HvfWindowsInstallRequest? = nil,
                           removedEntry: URL) -> Bool {
        let paths = [config.bundlePath, config.diskPath, config.isoPath, config.launchSpecPath,
                     config.runnerPath, config.handoffPath, config.sshKeyPath, config.leasesPath,
                     request?.isoPath, request?.driverPackageDir, request?.guestPayloadDirectory,
                     request?.guestPayloadManifest, request?.unattendedPath]
        for path in paths.compactMap({ $0 }) where !path.isEmpty {
            if LibraryDeletionPathReference.mayReference(path, removedEntry: removedEntry) { return true }
        }
        return false
    }

}
