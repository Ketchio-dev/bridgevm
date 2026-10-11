import Foundation

extension HvfProtectedTransfers {
    static func delete(_ config: VMConfig, rootURL: URL) -> Bool {
        (try? HvfDeletionOwnership.withOwnership(config: config, rootURL: rootURL) {
            VMLibrary.delete(config.slug, rootURL: rootURL)
        }) == true
    }
}
