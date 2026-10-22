import Foundation

/// The lane's private library holds only what this lane's app registered, under the
/// requested slug or not. T19 r1 registered `windows-11`, and a cleanup that looked only
/// under the requested slug leaked that key while reporting success. The product deletes
/// an imported key only when it equals the key opened from the source package, so another
/// VM's key under the same name is refused, and the refusal leaves cleanup unverified.
enum A9LaneRegistrations {
    static let limit = 16

    /// Directory names that hold a regular vm.json, sorted; nil when the library cannot be
    /// listed or holds more entries than one lane can create.
    static func slugs(request: A9ImportRequest, fileManager: FileManager) -> [String]? {
        let library = URL(fileURLWithPath: request.libraryRootPath, isDirectory: true)
        guard fileManager.fileExists(atPath: library.path) else { return [] }
        guard let entries = try? fileManager.contentsOfDirectory(atPath: library.path),
              entries.count <= limit else { return nil }
        var result: [String] = []
        for entry in entries.sorted() {
            let directory = library.appendingPathComponent(entry, isDirectory: true)
            guard real(directory, directory: true), real(directory.appendingPathComponent("vm.json"), directory: false) else { continue }
            guard safe(entry) else { return nil }
            result.append(entry)
        }
        return result
    }

    private static func safe(_ slug: String) -> Bool {
        !slug.isEmpty && slug != "." && slug != ".." && slug.utf8.count <= 255 && !slug.contains("/")
            && !slug.unicodeScalars.contains { CharacterSet.controlCharacters.contains($0) }
    }

    private static func real(_ url: URL, directory: Bool) -> Bool {
        guard let values = try? url.resourceValues(forKeys: [.isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey]),
              values.isSymbolicLink != true else { return false }
        return directory ? values.isDirectory == true : values.isRegularFile == true
    }
}
