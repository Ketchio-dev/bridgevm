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
                throw ScanFailure(errorDescription: "게스트 payload 폴더를 끝까지 읽을 수 없습니다.")
            }
            for child in children {
                entryCount += 1
                guard entryCount <= maximumEntries else {
                    throw ScanFailure(errorDescription: "게스트 payload는 8192개 항목 한도를 넘을 수 없습니다.")
                }
                let name = child.lastPathComponent
                let relative = directory.relativePath.isEmpty
                    ? name : directory.relativePath + "/" + name
                guard !name.isEmpty, !relative.contains("\n"), !relative.contains("\r") else {
                    throw ScanFailure(errorDescription: "게스트 payload 파일 이름이 안전하지 않습니다.")
                }
                let values: URLResourceValues
                do { values = try child.resourceValues(forKeys: keys) }
                catch {
                    throw ScanFailure(errorDescription: "게스트 payload 항목을 검사할 수 없습니다.")
                }
                guard values.isSymbolicLink != true else {
                    throw ScanFailure(errorDescription: "게스트 payload에는 심볼릭 링크를 둘 수 없습니다.")
                }
                if values.isDirectory == true {
                    pending.append((child, relative))
                    continue
                }
                guard values.isRegularFile == true, let rawSize = values.fileSize, rawSize >= 0 else {
                    throw ScanFailure(errorDescription: "게스트 payload에는 일반 파일만 둘 수 있습니다.")
                }
                let size = UInt64(rawSize)
                let addition = totalBytes.addingReportingOverflow(size)
                guard !addition.overflow else {
                    throw ScanFailure(errorDescription: "게스트 payload 크기를 안전하게 계산할 수 없습니다.")
                }
                totalBytes = addition.partialValue
                files.append(FileEntry(relativePath: relative, url: child, size: size))
                guard files.count <= maximumFiles, totalBytes <= maximumBytes else {
                    throw ScanFailure(errorDescription: "게스트 payload는 4096개 파일과 1 GiB 한도를 넘을 수 없습니다.")
                }
            }
        }
        return files
    }
}
