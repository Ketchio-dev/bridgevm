import Foundation

/// Snapshot commands run without the app's library admission. The app does not
/// load a VM with a relocation record (VMLibrary.scan), so no app snapshot can
/// reach it; refuse here too, before any directory, lease or helper work.
enum NativeCLISnapshotAdmission {
    static func engine(_ config: VMConfig, rootURL: URL) throws -> HvfEngineConfig {
        guard !VMRelocationJournal.isPending(config, rootURL: rootURL) else {
            throw NativeCLIError.unavailable(
                "VM relocation recovery is unresolved. Check both bundles and the registration before snapshot operations.")
        }
        guard let engine = HvfEngineConfig.libraryVM(config, rootURL: rootURL) else {
            throw NativeCLIError.unavailable("Cannot form the saved VM's HVF snapshot configuration.")
        }
        return engine
    }
}
