import SwiftUI
#if canImport(AppKit)
import AppKit
import UniformTypeIdentifiers
#endif
// MARK: - Create logic (shared helpers for Windows HVF creation)

extension VMLibrary {
    static let minimumImportedWindowsHVFDiskGiB: UInt64 = 64
    static let windowsHVFVarsBytes: UInt64 = 64 * 1024 * 1024
    static let maximumVMNameCharacters = 128
    static let maximumVMSlugBytes = 200

    static func normalizedVMName(_ rawName: String) -> String? {
        let name = rawName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty,
              name.count <= maximumVMNameCharacters,
              !name.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains),
              VMConfig.slugify(name).utf8.count <= maximumVMSlugBytes else { return nil }
        return name
    }

    static func windowsHVFImportError(targetDiskPath: String, varsPath: String) -> String? {
        let fm = FileManager.default
        let targetURL = URL(fileURLWithPath: targetDiskPath).resolvingSymlinksInPath().standardizedFileURL
        let varsURL = URL(fileURLWithPath: varsPath).resolvingSymlinksInPath().standardizedFileURL
        guard isReadableRegularFile(targetURL.path) else {
            return "설치된 Windows RAW 디스크 파일을 찾을 수 없습니다."
        }
        let targetBytes = ((try? fm.attributesOfItem(atPath: targetURL.path)[.size] as? NSNumber)?.uint64Value) ?? 0
        guard targetBytes > 0 else { return "Windows RAW 디스크가 비어 있습니다." }
        if let error = HvfImportedDiskFormat.detect(path: targetURL.path)?.importError { return error.description }
        if varsPath.isEmpty { return nil }
        guard isReadableRegularFile(varsURL.path) else {
            return "이 VM과 함께 사용한 UEFI vars 파일을 찾을 수 없습니다."
        }
        guard targetURL != varsURL else {
            return "Windows RAW 디스크와 UEFI vars는 서로 다른 파일이어야 합니다."
        }
        let varsBytes = ((try? fm.attributesOfItem(atPath: varsURL.path)[.size] as? NSNumber)?.uint64Value) ?? 0
        guard varsBytes == windowsHVFVarsBytes else {
            return "UEFI vars는 정확히 64 MiB여야 합니다 (현재 \(varsBytes)바이트)."
        }
        return nil
    }

    static func reserveDestination(
        _ base: String,
        storageBase: URL,
        libraryRoot: URL = root
    ) -> (slug: String, root: URL)? {
        let baseSlug = VMConfig.slugify(base)
        let fm = FileManager.default
        do {
            try fm.createDirectory(at: libraryRoot, withIntermediateDirectories: true)
            try fm.createDirectory(at: storageBase, withIntermediateDirectories: true)
        } catch {
            return nil
        }
        let libraryEntries: [URL]
        do {
            libraryEntries = try fm.contentsOfDirectory(at: libraryRoot, includingPropertiesForKeys: nil)
        } catch {
            return nil
        }
        // Include every on-disk entry, not only successfully decoded VMs. A
        // corrupt or noncanonical registration must never be overwritten by a
        // newly created VM that happens to normalize to the same slug.
        let existing = Set(libraryEntries.flatMap {
            [$0.lastPathComponent, VMConfig.slugify($0.lastPathComponent)]
        })
        var slug = baseSlug
        var n = 2
        while true {
            let destination = storageBase.appendingPathComponent(slug, isDirectory: true)
            if existing.contains(slug) || fm.fileExists(atPath: destination.path) {
                slug = "\(baseSlug)-\(n)"; n += 1
                continue
            }
            do {
                try fm.createDirectory(at: destination, withIntermediateDirectories: false)
                return (slug, destination)
            } catch {
                if fm.fileExists(atPath: destination.path) {
                    slug = "\(baseSlug)-\(n)"; n += 1
                    continue
                }
                return nil
            }
        }
    }

    static func cloneOrCopyFile(from source: String, to destination: String) -> Bool {
        let fm = FileManager.default
        let clone = Shell.run("/bin/cp", ["-c", source, destination])
        if clone.code == 0 { return true }
        try? fm.removeItem(atPath: destination)
        do {
            try fm.copyItem(atPath: source, toPath: destination)
            return true
        } catch {
            return false
        }
    }

    static func cloneOrCopyDirectory(from source: String, to destination: String) -> Bool {
        let fm = FileManager.default
        let clone = Shell.run("/bin/cp", ["-c", "-R", source, destination])
        if clone.code == 0 { return true }
        try? fm.removeItem(atPath: destination)
        do {
            try fm.copyItem(atPath: source, toPath: destination)
            return true
        } catch {
            return false
        }
    }

    static func isReadableRegularFile(_ path: String) -> Bool {
        let url = URL(fileURLWithPath: path).resolvingSymlinksInPath().standardizedFileURL
        guard (try? url.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true else {
            return false
        }
        return FileManager.default.isReadableFile(atPath: url.path)
    }

    static func isReadableDirectory(_ path: String) -> Bool {
        var isDirectory: ObjCBool = false
        return FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory)
            && isDirectory.boolValue
            && FileManager.default.isReadableFile(atPath: path)
    }

    static func isSameOrDescendant(_ candidate: URL, of ancestor: URL) -> Bool {
        let candidateComponents = candidate.resolvingSymlinksInPath().standardizedFileURL.pathComponents
        let ancestorComponents = ancestor.resolvingSymlinksInPath().standardizedFileURL.pathComponents
        guard candidateComponents.count >= ancestorComponents.count else { return false }
        return Array(candidateComponents.prefix(ancestorComponents.count)) == ancestorComponents
    }

    static func windowsHVFInjectionError(requested: Bool) -> String? {
        guard requested else { return nil }
        return HvfWindowsDriverPreflight.message(for: HvfWindowsDriverPreflight.provenanceBlocker)
    }
}
