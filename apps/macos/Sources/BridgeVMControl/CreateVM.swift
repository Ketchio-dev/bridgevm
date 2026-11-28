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
    /// Shown when `normalizedVMName` refuses a name that is not empty.
    static let vmNameRule = "Use 1–\(maximumVMNameCharacters) characters without control characters. If the name is long, shorten it to fit the file name limit."

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
        if isInstallerISO(targetURL.path) { return installerISORefusal }
        if varsPath.isEmpty { return nil }
        guard isReadableRegularFile(varsURL.path) else {
            return "Choose the matching UEFI variables file or leave it empty."
        }
        guard targetURL != varsURL else {
            return "Choose separate files for the Windows disk and UEFI variables."
        }
        let varsBytes = ((try? fm.attributesOfItem(atPath: varsURL.path)[.size] as? NSNumber)?.uint64Value) ?? 0
        guard varsBytes == windowsHVFVarsBytes else {
            return "The UEFI variables file must be exactly 64 MiB. Choose the one that came with this Windows disk, or leave it empty."
        }
        return nil
    }

    static let installerISORefusal = "This is a Windows installer ISO, not a disk with Windows installed. Install Windows from it instead."

    /// An ISO 9660 volume descriptor ("CD001" at byte 0x8001), as on every Windows installer ISO.
    /// Called after the disk check, so the path is a readable regular file.
    private static func isInstallerISO(_ path: String) -> Bool {
        guard let handle = try? FileHandle(forReadingFrom: URL(fileURLWithPath: path)) else { return false }
        defer { try? handle.close() }
        guard (try? handle.seek(toOffset: 0x8001)) != nil else { return false }
        return (try? handle.read(upToCount: 5)) == Data("CD001".utf8)
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

    /// Clones on APFS (no space used, measured) and copies otherwise, with no time limit and keeping a
    /// sparse disk's holes. FileManager's copy across volumes wrote a 2 GiB file holding 4 MiB of data
    /// as 2 GiB (measured), so duplicating a VM stored on another drive wrote the disk's whole size to
    /// the library volume. `clone: false` forces the copy path, as across volumes.
    static func cloneOrCopyFile(from source: String, to destination: String, clone: Bool = true) -> Bool {
        guard (try? FileManager.default.attributesOfItem(atPath: destination)) == nil else { return false }
        var flags = COPYFILE_ALL | COPYFILE_RECURSIVE | COPYFILE_DATA_SPARSE
        if clone { flags |= COPYFILE_CLONE }
        guard copyfile(source, destination, nil, copyfile_flags_t(flags)) == 0 else {
            try? FileManager.default.removeItem(atPath: destination)
            return false
        }
        return true
    }

    static func cloneOrCopyDirectory(from source: String, to destination: String) -> Bool {
        cloneOrCopyFile(from: source, to: destination)
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
