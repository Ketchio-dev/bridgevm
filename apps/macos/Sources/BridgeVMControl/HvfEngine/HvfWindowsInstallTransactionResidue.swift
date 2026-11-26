import Darwin
import Foundation

/// An unresolved install transaction keeps a journal bound to the current bundle
/// path. Launch and relocation both wait for it; nothing here rewrites the journal.
enum HvfWindowsInstallTransactionResidue {
    /// Clone copies the journal too, and the copy fails validation at its new path.
    static func refusal(_ verb: String) -> String {
        "BridgeVM couldn’t \(verb) this VM because Windows installation cleanup is incomplete. Refresh the library and retry. If it’s still blocked, check the saved installation record. Keep the VM files and avoid running the installer again."
    }

    /// Any residual entry or failed lookup counts. A missing journal inside a
    /// partly removed directory does not authorize launch or relocation.
    static func isPresent(_ config: VMConfig, rootURL: URL) -> Bool {
        let transaction = HvfWindowsInstallFinalization.paths(slug: config.slug, libraryRoot: rootURL,
                                                              bundlePath: config.bundlePath).transaction
        var info = stat()
        return lstat(transaction.path, &info) == 0 || errno != ENOENT
    }
}

extension LibraryModel {
    func transactionResidueRefusal(for cfg: VMConfig, action: LibraryFileAction) -> String? {
        guard action != .deletion, HvfWindowsInstallTransactionResidue.isPresent(cfg, rootURL: rootURL) else { return nil }
        return HvfWindowsInstallTransactionResidue.refusal(action == .move ? "move" : "duplicate")
    }
}
