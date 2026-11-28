import Foundation

/// The library deletion confirmation dialog's wording. It names the bundle the
/// deletion removes, never a stale saved path the user would find empty. A running
/// VM's Delete is refused before this dialog, so it gives no shut-down advice.
struct LibraryDeletionPrompt: Equatable {
    let confirmTitle: String
    let message: String

    init(config: VMConfig, rootURL: URL) {
        if let target = VMLibrary.deletionTarget(for: config, rootURL: rootURL) {
            confirmTitle = "Delete \(config.name) and Its Disk"
            message = "This permanently deletes the VM settings, the virtual disk and any snapshots at \(Self.shown(target.path))."
        } else {
            confirmTitle = "Remove \(config.name) from Library"
            message = "This removes the VM from your library. Its files stay in \(Self.shown(config.bundlePath))."
        }
    }

    /// The path stays in the message so people can find the files; ~ keeps it short.
    static func shown(_ path: String) -> String { (path as NSString).abbreviatingWithTildeInPath }
}
