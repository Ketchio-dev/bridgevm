import Foundation

enum HvfWindowsGuestPayloadTree {
    static let maximumFiles = 4_096
    static let maximumEntries = 8_192
    static let maximumBytes: UInt64 = 1_024 * 1_024 * 1_024

    struct FileEntry {
        let relativePath: String
        let url: URL
        let size: UInt64
    }

    struct ScanFailure: LocalizedError {
        let errorDescription: String?
    }

    static func scan(_ root: URL, fileManager: FileManager = .default) throws -> [FileEntry] {
        let keys: Set<URLResourceKey> = [
            .isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey
        ]
        var pending = [(url: root, relativePath: "")]
        var files: [FileEntry] = []
        var entryCount = 0
        var totalBytes: UInt64 = 0

        while let directory = pending.popLast() {
            let children: [URL]
            do {
                children = try fileManager.contentsOfDirectory(
                    at: directory.url, includingPropertiesForKeys: Array(keys), options: [])
            } catch {
                throw ScanFailure(errorDescription: "BridgeVM couldn’t read all the files in the Windows driver folder.")
            }
            for child in children {
                entryCount += 1
                guard entryCount <= maximumEntries else {
                    throw ScanFailure(errorDescription: "The driver folder exceeds the limit of 8192 entries.")
                }
                let name = child.lastPathComponent
                let relative = directory.relativePath.isEmpty
                    ? name : directory.relativePath + "/" + name
                guard !name.isEmpty, !relative.contains("\n"), !relative.contains("\r") else {
                    throw ScanFailure(errorDescription: "The driver folder contains an unsupported file name.")
                }
                let values: URLResourceValues
                do { values = try child.resourceValues(forKeys: keys) }
                catch {
                    throw ScanFailure(errorDescription: "BridgeVM couldn’t inspect an entry in the driver folder.")
                }
                guard values.isSymbolicLink != true else {
                    throw ScanFailure(errorDescription: "Remove symbolic links from the Windows driver folder.")
                }
                if values.isDirectory == true {
                    pending.append((child, relative))
                    continue
                }
                guard values.isRegularFile == true, let rawSize = values.fileSize, rawSize >= 0 else {
                    throw ScanFailure(errorDescription: "The driver folder contains an entry that is neither a file nor a folder.")
                }
                let size = UInt64(rawSize)
                let addition = totalBytes.addingReportingOverflow(size)
                guard !addition.overflow else {
                    throw ScanFailure(errorDescription: "BridgeVM couldn’t determine the size of the Windows driver files.")
                }
                totalBytes = addition.partialValue
                files.append(FileEntry(relativePath: relative, url: child, size: size))
                guard files.count <= maximumFiles, totalBytes <= maximumBytes else {
                    throw ScanFailure(errorDescription: "The driver folder exceeds the limit of 4096 files or 1 GiB.")
                }
            }
        }
        return files
    }
}
