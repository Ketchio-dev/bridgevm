import Foundation

enum VMRelocationDiskPath {
    /// Capture the selected target before moving. An external alias can point
    /// into the bundle even when its saved spelling has no bundle prefix.
    static func rebased(_ path: String, source: URL, destination: URL) -> String? {
        guard !path.isEmpty else { return nil }
        let sourceParts = source.resolvingSymlinksInPath().standardizedFileURL.pathComponents
        let selectedParts = URL(fileURLWithPath: path).resolvingSymlinksInPath().standardizedFileURL.pathComponents
        guard selectedParts.starts(with: sourceParts) else { return nil }
        return selectedParts.dropFirst(sourceParts.count).reduce(destination) {
            $0.appendingPathComponent($1)
        }.path
    }
}
