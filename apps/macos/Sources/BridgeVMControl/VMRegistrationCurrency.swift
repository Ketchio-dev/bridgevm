import Foundation

extension VMLibrary {
    /// A handle may rewrite only the unchanged, still-present registration it was
    /// built from; a moved, replaced or removed VM is never reverted or resurrected.
    static func isCurrentRegistration(_ config: VMConfig, rootURL: URL) -> Bool {
        var registered = config
        registered.id = config.slug
        return (try? NativeLibraryReader.readConfig(rootURL: rootURL, id: config.slug)) == registered
    }
}
