import Foundation

struct HvfScreenshotFingerprint: Equatable {
    let path: String
    let device: UInt64
    let inode: UInt64
    let size: UInt64
    let modificationTime: TimeInterval
}

enum HvfScreenshotSource {
    static func resolve(in evidenceDirectory: URL, fileManager: FileManager = .default) -> (URL, HvfScreenshotFingerprint)? {
        let live = evidenceDirectory.appendingPathComponent("display.ppm")
        if let fingerprint = fingerprint(of: live, fileManager: fileManager) {
            return (live, fingerprint)
        }

        let directory = evidenceDirectory.appendingPathComponent("ramfb", isDirectory: true)
        guard let urls = try? fileManager.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: nil,
            options: [.skipsHiddenFiles]
        ) else { return nil }

        return urls
            .filter { $0.pathExtension.lowercased() == "ppm" }
            .compactMap { url in fingerprint(of: url, fileManager: fileManager).map { (url, $0) } }
            .max { $0.1.modificationTime < $1.1.modificationTime }
    }

    static func fingerprint(of url: URL, fileManager: FileManager = .default) -> HvfScreenshotFingerprint? {
        guard let attributes = try? fileManager.attributesOfItem(atPath: url.path),
              let type = attributes[.type] as? FileAttributeType,
              type == .typeRegular,
              let size = (attributes[.size] as? NSNumber)?.uint64Value,
              let modificationDate = attributes[.modificationDate] as? Date else { return nil }
        return HvfScreenshotFingerprint(
            path: url.standardizedFileURL.path,
            device: (attributes[.systemNumber] as? NSNumber)?.uint64Value ?? 0,
            inode: (attributes[.systemFileNumber] as? NSNumber)?.uint64Value ?? 0,
            size: size,
            modificationTime: modificationDate.timeIntervalSinceReferenceDate
        )
    }
}
