import Foundation

/// Requests keep their fixed legacy locator; the product now creates timestamped siblings.
enum T17SnapshotLocation {
    static func resolve(_ legacy: URL, fileManager: FileManager = .default) throws -> URL {
        let parent = legacy.deletingLastPathComponent()
        guard parent.standardizedFileURL == parent.resolvingSymlinksInPath().standardizedFileURL else {
            throw T17Blocker(code: "snapshot-unavailable", detail: "snapshot directory crosses a symbolic link")
        }
        let entries = try fileManager.contentsOfDirectory(at: parent, includingPropertiesForKeys: nil)
        let snapshots = entries.filter { url in
            let name = url.lastPathComponent
            guard name == "latest.snapshot" || name.range(of: #"^[0-9]{8}-[0-9]{6}\.snapshot$"#, options: .regularExpression) != nil,
                  let values = try? url.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey]) else { return false }
            return values.isDirectory == true && values.isSymbolicLink != true
        }
        guard let newest = snapshots.max(by: { date($0) == date($1) ? $0.lastPathComponent < $1.lastPathComponent : date($0) < date($1) }) else {
            throw T17Blocker(code: "snapshot-unavailable", detail: "no published snapshot")
        }
        return parent.appendingPathComponent(newest.lastPathComponent).standardizedFileURL
    }

    private static func date(_ url: URL) -> Date {
        (try? url.appendingPathComponent("manifest.json").resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast
    }
}
