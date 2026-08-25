import Foundation

extension VMLibrary {
    static let minimumImportedWindowsHVFDiskGiB: UInt64 = 64
    static let windowsHVFVarsBytes: UInt64 = 64 * 1024 * 1024
    static let maximumVMNameCharacters = 128
    static let maximumVMSlugBytes = 200

    static func normalizedVMName(_ rawName: String) -> String? {
        let name = rawName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty, name.count <= maximumVMNameCharacters,
              !name.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains),
              VMConfig.slugify(name).utf8.count <= maximumVMSlugBytes else { return nil }
        return name
    }

    static func windowsHVFImportError(targetDiskPath: String, varsPath: String) -> String? {
        let fm = FileManager.default
        let target = URL(fileURLWithPath: targetDiskPath).resolvingSymlinksInPath().standardizedFileURL
        let vars = URL(fileURLWithPath: varsPath).resolvingSymlinksInPath().standardizedFileURL
        guard isReadableRegularFile(target.path) else {
            return "설치된 Windows RAW 디스크 파일을 찾을 수 없습니다."
        }
        guard isReadableRegularFile(vars.path) else {
            return "이 VM과 함께 사용한 UEFI vars 파일을 찾을 수 없습니다."
        }
        guard target != vars else { return "Windows RAW 디스크와 UEFI vars는 서로 다른 파일이어야 합니다." }
        let targetBytes = ((try? fm.attributesOfItem(atPath: target.path)[.size] as? NSNumber)?.uint64Value) ?? 0
        guard targetBytes > 0 else { return "Windows RAW 디스크가 비어 있습니다." }
        let varsBytes = ((try? fm.attributesOfItem(atPath: vars.path)[.size] as? NSNumber)?.uint64Value) ?? 0
        guard varsBytes == windowsHVFVarsBytes else {
            return "UEFI vars는 정확히 64 MiB여야 합니다 (현재 \(varsBytes)바이트)."
        }
        if let handle = try? FileHandle(forReadingFrom: target) {
            defer { try? handle.close() }
            let header = (try? handle.read(upToCount: 8)) ?? Data()
            if header.starts(with: Data([0x51, 0x46, 0x49, 0xfb])) {
                return "QCOW2 이미지는 가져올 수 없습니다. 설치된 RAW 디스크를 선택하세요."
            }
            if header.starts(with: Data("vhdxfile".utf8)) {
                return "VHDX 이미지는 가져올 수 없습니다. 설치된 RAW 디스크를 선택하세요."
            }
        }
        return nil
    }

    static func reserveDestination(
        _ base: String, storageBase: URL, libraryRoot: URL = root
    ) -> (slug: String, root: URL)? {
        let baseSlug = VMConfig.slugify(base); let fm = FileManager.default
        do {
            try fm.createDirectory(at: libraryRoot, withIntermediateDirectories: true)
            try fm.createDirectory(at: storageBase, withIntermediateDirectories: true)
        } catch { return nil }
        guard let entries = try? fm.contentsOfDirectory(
            at: libraryRoot, includingPropertiesForKeys: nil) else { return nil }
        let existing = Set(entries.flatMap {
            [$0.lastPathComponent, VMConfig.slugify($0.lastPathComponent)]
        })
        var slug = baseSlug; var n = 2
        while true {
            let destination = storageBase.appendingPathComponent(slug, isDirectory: true)
            if existing.contains(slug) || fm.fileExists(atPath: destination.path) {
                slug = "\(baseSlug)-\(n)"; n += 1; continue
            }
            do {
                try fm.createDirectory(at: destination, withIntermediateDirectories: false)
                return (slug, destination)
            } catch {
                if fm.fileExists(atPath: destination.path) {
                    slug = "\(baseSlug)-\(n)"; n += 1; continue
                }
                return nil
            }
        }
    }

    static func cloneOrCopyFile(from source: String, to destination: String) -> Bool {
        let fm = FileManager.default
        if Shell.run("/bin/cp", ["-c", source, destination]).code == 0 { return true }
        try? fm.removeItem(atPath: destination)
        do { try fm.copyItem(atPath: source, toPath: destination); return true }
        catch { return false }
    }

    static func cloneOrCopyDirectory(from source: String, to destination: String) -> Bool {
        let fm = FileManager.default
        if Shell.run("/bin/cp", ["-c", "-R", source, destination]).code == 0 { return true }
        try? fm.removeItem(atPath: destination)
        do { try fm.copyItem(atPath: source, toPath: destination); return true }
        catch { return false }
    }

    static func isReadableRegularFile(_ path: String) -> Bool {
        let url = URL(fileURLWithPath: path).resolvingSymlinksInPath().standardizedFileURL
        return (try? url.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true
            && FileManager.default.isReadableFile(atPath: url.path)
    }

    static func isReadableDirectory(_ path: String) -> Bool {
        var isDirectory: ObjCBool = false
        return FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory)
            && isDirectory.boolValue && FileManager.default.isReadableFile(atPath: path)
    }

    static func isSameOrDescendant(_ candidate: URL, of ancestor: URL) -> Bool {
        let candidateParts = candidate.resolvingSymlinksInPath().standardizedFileURL.pathComponents
        let ancestorParts = ancestor.resolvingSymlinksInPath().standardizedFileURL.pathComponents
        return candidateParts.count >= ancestorParts.count
            && Array(candidateParts.prefix(ancestorParts.count)) == ancestorParts
    }

    static func rewriteCloneMetadata(
        at path: String, oldBundlePath: String, newBundlePath: String, newName: String
    ) -> Bool {
        guard var object = JSONFile.loadDict(path) else { return false }
        func rewrite(_ value: Any) -> Any {
            if let value = value as? String {
                return value.replacingOccurrences(of: oldBundlePath, with: newBundlePath)
            }
            if let value = value as? [Any] { return value.map(rewrite) }
            if let value = value as? [String: Any] { return value.mapValues(rewrite) }
            return value
        }
        object = rewrite(object) as? [String: Any] ?? object; object["vm_name"] = newName
        return JSONFile.writeDict(object, to: path)
    }

    static func growSparseFileIfNeeded(at path: String, minimumBytes: UInt64) -> Bool {
        guard let size = (try? FileManager.default.attributesOfItem(
            atPath: path)[.size] as? NSNumber)?.uint64Value else { return false }
        guard size < minimumBytes else { return true }
        guard let handle = try? FileHandle(forWritingTo: URL(fileURLWithPath: path)) else { return false }
        defer { try? handle.close() }
        do { try handle.truncate(atOffset: minimumBytes); return true }
        catch { return false }
    }
}
