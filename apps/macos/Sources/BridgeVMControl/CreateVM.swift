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

    static func registrationSlug(for name: String) -> String {
        let slug = VMConfig.slugify(name)
        // Keep accepted ID spellings. A decomposed name may exceed the byte limit only in that form.
        return slug.utf8.count <= maximumVMSlugBytes ? slug : slug.precomposedStringWithCanonicalMapping
    }

    static func normalizedDisplayName(_ rawName: String) -> String? {
        let name = rawName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty,
              name.count <= maximumVMNameCharacters,
              !name.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains) else { return nil }
        return name
    }

    static func normalizedVMName(_ rawName: String) -> String? {
        guard let name = normalizedDisplayName(rawName),
              registrationSlug(for: name).utf8.count <= maximumVMSlugBytes else { return nil }
        return name
    }

    static func windowsHVFImportError(targetDiskPath: String, varsPath: String) -> String? {
        let fm = FileManager.default
        let targetURL = URL(fileURLWithPath: targetDiskPath).resolvingSymlinksInPath().standardizedFileURL
        let varsURL = URL(fileURLWithPath: varsPath).resolvingSymlinksInPath().standardizedFileURL
        guard isReadableRegularFile(targetURL.path) else {
            return "Choose a readable disk file with Windows installed."
        }
        let targetBytes = ((try? fm.attributesOfItem(atPath: targetURL.path)[.size] as? NSNumber)?.uint64Value) ?? 0
        guard targetBytes > 0 else { return "The Windows disk is empty." }
        if let error = HvfImportedDiskFormat.detect(path: targetURL.path)?.importError { return error.description }
        if varsPath.isEmpty { return nil }
        guard isReadableRegularFile(varsURL.path) else {
            return "Choose the matching UEFI variables file or leave it empty."
        }
        guard targetURL != varsURL else {
            return "Choose separate files for the Windows disk and UEFI variables."
        }
        let varsBytes = ((try? fm.attributesOfItem(atPath: varsURL.path)[.size] as? NSNumber)?.uint64Value) ?? 0
        guard varsBytes == windowsHVFVarsBytes else {
            return "The UEFI variables file must be exactly 64 MiB. The selected file contains \(varsBytes) bytes."
        }
        return nil
    }

    static func reserveDestination(
        _ base: String,
        storageBase: URL,
        libraryRoot: URL = root
    ) -> (slug: String, root: URL)? {
        let baseSlug = registrationSlug(for: base)
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
