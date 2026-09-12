import Foundation

/// A pending record is recovery evidence, never proof that either copy is complete.
enum VMRelocationJournal {
    struct Record: Codable {
        let schema: String
        let original: VMConfig
        let destination: VMConfig
    }

    static func url(_ config: VMConfig, rootURL: URL) -> URL {
        rootURL.appendingPathComponent(config.slug, isDirectory: true)
            .appendingPathComponent(".relocation-pending.json")
    }

    static func isPending(_ config: VMConfig, rootURL: URL) -> Bool {
        do {
            _ = try FileManager.default.attributesOfItem(atPath: url(config, rootURL: rootURL).path)
            return true
        } catch {
            let error = error as NSError
            return !(error.domain == NSCocoaErrorDomain &&
                (error.code == CocoaError.fileNoSuchFile.rawValue ||
                 error.code == CocoaError.fileReadNoSuchFile.rawValue))
        }
    }

    static func begin(original: VMConfig, moved: VMConfig, rootURL: URL) throws -> URL {
        let recordURL = url(original, rootURL: rootURL)
        let directory = recordURL.deletingLastPathComponent()
        let fm = FileManager.default
        guard !isPending(original, rootURL: rootURL),
              VMRelocationPaths.isSafe(source: URL(fileURLWithPath: original.bundlePath), destination: directory),
              VMRelocationPaths.isSafe(source: URL(fileURLWithPath: moved.bundlePath), destination: directory),
              try fm.attributesOfItem(atPath: directory.path)[.type] as? FileAttributeType == .typeDirectory else {
            throw CocoaError(.fileWriteUnknown)
        }
        let record = Record(schema: "bridgevm.relocation-pending.v1", original: original, destination: moved)
        try JSONEncoder().encode(record).write(to: recordURL, options: [.atomic])
        return recordURL
    }
}
