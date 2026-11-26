import Foundation

/// All product deletion paths remove an entire library entry, which can also
/// contain a different saved VM's relocated bundle or installation inputs.
/// Check immediately before removal; this is not a concurrent-writer lock.
enum LibraryDeletionReferences {
    static func delete(_ config: VMConfig, rootURL: URL) -> Bool {
        guard refusal(for: config, rootURL: rootURL) == nil else { return false }
        return VMLibrary.delete(config.slug, rootURL: rootURL)
    }

    static func refusal(for config: VMConfig, rootURL: URL) -> String? {
        do {
            try HvfPendingDeletionReferences.validate(config, rootURL: rootURL)
            return nil
        } catch {
            return "Another VM may use files in this library entry. Move that VM or check its registration before deleting this one."
        }
    }

    static func failureMessage(for config: VMConfig, rootURL: URL) -> String {
        refusal(for: config, rootURL: rootURL)
            ?? "The library entry for \(config.name) could not be removed from disk."
    }
}
