import Foundation

extension HvfProtectedTransfers {
    static func delete(_ config: VMConfig, rootURL: URL) -> Bool {
        if let unavailable = HvfUnavailableMediaDeletion.delete(config, rootURL: rootURL) { return unavailable }
        return (try? HvfDeletionOwnership.withOwnership(config: config, rootURL: rootURL) {
            VMLibrary.delete(config.slug, rootURL: rootURL) }) == true
    }
}
