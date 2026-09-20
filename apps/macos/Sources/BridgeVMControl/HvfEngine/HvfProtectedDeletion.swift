import Foundation

extension HvfProtectedTransfers {
    static func delete(_ config: VMConfig, rootURL: URL) -> Bool {
        (try? HvfMediaLeaseSession.withOwnership(config: config) {
            VMLibrary.delete(config.slug, rootURL: rootURL)
        }) == true
    }
}
