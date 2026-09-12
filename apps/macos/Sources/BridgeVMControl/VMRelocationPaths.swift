import Foundation

enum VMRelocationPaths {
    static func isSafe(source: URL, destination: URL) -> Bool {
        guard let source = prospectiveDirectory(source),
              let destination = prospectiveDirectory(destination), source.path != "/" else { return false }
        return destination.path != source.path && !destination.path.hasPrefix(source.path + "/")
    }

    /// URL resolution alone leaves symlinks unresolved when a suffix is absent.
    /// This is a preflight check, not protection against hostile concurrent renames.
    static func prospectiveDirectory(_ url: URL, fileManager: FileManager = .default) -> URL? {
        guard url.isFileURL else { return nil }
        var ancestor = url.standardizedFileURL
        var suffix: [String] = []
        for _ in 0..<1024 {
            var directory: ObjCBool = false
            if fileManager.fileExists(atPath: ancestor.path, isDirectory: &directory) {
                guard directory.boolValue else { return nil }
                var resolved = ancestor.resolvingSymlinksInPath().standardizedFileURL
                for component in suffix.reversed() {
                    resolved.appendPathComponent(component, isDirectory: true)
                }
                return resolved
            }
            // A dangling or cyclic link is not an absent directory to create.
            if (try? fileManager.destinationOfSymbolicLink(atPath: ancestor.path)) != nil { return nil }
            let parent = ancestor.deletingLastPathComponent()
            guard parent.path != ancestor.path, !ancestor.lastPathComponent.isEmpty else { return nil }
            suffix.append(ancestor.lastPathComponent)
            ancestor = parent
        }
        return nil
    }
}
