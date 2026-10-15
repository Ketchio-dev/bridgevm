import Foundation

enum HvfCloneTestMedia {
    static func write(disk: URL, bytes: Data) throws {
        let bundle = disk.deletingLastPathComponent().deletingLastPathComponent()
        let vars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
        for (path, contents) in [(disk, bytes), (vars, Data("owned clone vars".utf8))] {
            try FileManager.default.createDirectory(at: path.deletingLastPathComponent(), withIntermediateDirectories: true)
            try contents.write(to: path)
        }
    }
}
