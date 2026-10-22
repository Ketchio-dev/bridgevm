import Foundation

/// The slugs whose imported key a lane cleanup must account for: the requested slug and every real directory in the
/// lane's private library, published or not. The product installs an imported key before it publishes vm.json, so an
/// interrupted import leaves a key under a bare directory, or under the requested slug with nothing published, and
/// published registrations alone (A9LaneRegistrations) miss both. The install source cache is not a VM.
enum A9KeyCleanupCandidates {
    /// Sorted; nil when the library cannot be listed, holds more than one lane creates, or has a non-account name.
    static func slugs(request: A9ImportRequest, fileManager: FileManager) -> [String]? {
        let library = URL(fileURLWithPath: request.libraryRootPath, isDirectory: true)
        guard fileManager.fileExists(atPath: library.path) else { return [request.vmSlug] }
        guard let entries = try? fileManager.contentsOfDirectory(atPath: library.path),
              entries.count <= A9LaneRegistrations.limit else { return nil }
        var result: Set<String> = [request.vmSlug]
        for entry in entries {
            let directory = library.appendingPathComponent(entry, isDirectory: true)
            let values = try? directory.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            guard values?.isSymbolicLink == false, values?.isDirectory == true,
                  !(entry == "Derived" && unregistered(directory)) else { continue }
            guard A9LaneRegistrations.safe(entry) else { return nil }
            result.insert(entry)
        }
        return result.sorted()
    }

    private static func unregistered(_ directory: URL) -> Bool {
        (try? FileManager.default.attributesOfItem(atPath: directory.appendingPathComponent("vm.json").path)) == nil
    }
}
