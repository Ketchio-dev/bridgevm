import Foundation

extension VMLibrary {
    /// Deletion recursively removes `<root>/<slug>/`. A stale saved path must not
    /// hide an installer-created `bundle.vmbridge` or first-run imported `bundle`.
    /// External targets of symlinks inside the entry survive recursive deletion.
    static func deletionTarget(for config: VMConfig, rootURL: URL = root) -> URL? {
        let rawEntry = rootURL.appendingPathComponent(config.slug, isDirectory: true)
        if (try? rawEntry.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) == true { return nil }
        let entry = rawEntry.resolvingSymlinksInPath().standardizedFileURL
        let saved = URL(fileURLWithPath: config.bundlePath)
        let bundle = saved.resolvingSymlinksInPath().standardizedFileURL
        let entryPrefix = entry.path.hasSuffix("/") ? entry.path : entry.path + "/"
        if bundle.path == entry.path || bundle.path.hasPrefix(entryPrefix) { return saved }
        return ["bundle.vmbridge", "bundle"].lazy
            .map { rawEntry.appendingPathComponent($0, isDirectory: true) }
            .first {
                let values = try? $0.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
                return values?.isDirectory == true && values?.isSymbolicLink != true
            }
    }
}
