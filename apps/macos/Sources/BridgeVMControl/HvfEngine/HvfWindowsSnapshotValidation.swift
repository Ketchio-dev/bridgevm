import Foundation

extension HvfWindowsSnapshotCommand {
    static func regularFile(_ url: URL) -> Bool {
        guard let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey]) else { return false }
        return values.isRegularFile == true && values.isSymbolicLink != true
    }

    static func regularDirectory(_ url: URL) -> Bool {
        guard let values = try? url.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey]) else { return false }
        return values.isDirectory == true && values.isSymbolicLink != true
    }

    static func canonical(_ url: URL) -> Bool {
        url.standardizedFileURL.path == url.resolvingSymlinksInPath().standardizedFileURL.path
    }

    static func fileSize(_ url: URL) throws -> UInt64 {
        let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
        guard let size = attributes[.size] as? NSNumber else { throw failure("media size is unavailable") }
        return size.uint64Value
    }

    static func failure(_ message: String) -> NSError {
        NSError(domain: "BridgeVM.HvfWindowsSnapshot", code: 1,
                userInfo: [NSLocalizedDescriptionKey: message.isEmpty ? "snapshot operation failed" : message])
    }
}
