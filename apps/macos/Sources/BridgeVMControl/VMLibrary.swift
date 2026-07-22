import Foundation

struct VMLibraryIssue: Identifiable, Equatable {
    let path: String
    let message: String
    var id: String { path }
}

struct VMLibraryScan {
    let configs: [VMConfig]
    let issues: [VMLibraryIssue]
}

enum VMLibraryDeletionImpact: Equatable {
    case managedBundleDeleted
    case registrationOnly
}

/// Manages MANY VMs on disk: each VM is a `vm.json` under the library dir.
/// This is the multi-VM substrate — the app enumerates, creates, and deletes
/// VMs here, and each VMConfig points at its own bundle (disks + metadata).
enum VMLibrary {
    static let maximumConfigBytes = 1_048_576

    /// ~/Library/Application Support/BridgeVM/vms/<slug>/vm.json
    static var root: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support")
        return base.appendingPathComponent("BridgeVM/vms", isDirectory: true)
    }

    /// Enumerate all VMs in the library (sorted by display name).
    static func scan(rootURL: URL = root) -> VMLibraryScan {
        let fm = FileManager.default
        do {
            try fm.createDirectory(at: rootURL, withIntermediateDirectories: true)
        } catch {
            return VMLibraryScan(configs: [], issues: [
                VMLibraryIssue(path: rootURL.path, message: "라이브러리 디렉터리를 열 수 없습니다: \(error.localizedDescription)")
            ])
        }
        let entries: [URL]
        do {
            entries = try fm.contentsOfDirectory(
                at: rootURL,
                includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey]
            )
        } catch {
            return VMLibraryScan(configs: [], issues: [
                VMLibraryIssue(path: rootURL.path, message: "라이브러리 목록을 읽을 수 없습니다: \(error.localizedDescription)")
            ])
        }
        var decoded: [(directory: URL, config: VMConfig)] = []
        var issues: [VMLibraryIssue] = []
        for dir in entries {
            let values: URLResourceValues
            do {
                values = try dir.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            } catch {
                issues.append(VMLibraryIssue(path: dir.path, message: "항목 종류를 확인할 수 없습니다: \(error.localizedDescription)"))
                continue
            }
            if values.isSymbolicLink == true {
                issues.append(VMLibraryIssue(path: dir.path, message: "심볼릭 링크 VM 항목은 안전을 위해 불러오지 않았습니다."))
                continue
            }
            guard values.isDirectory == true else { continue }
            let canonicalID = VMConfig.slugify(dir.lastPathComponent)
            guard dir.lastPathComponent == canonicalID else {
                issues.append(VMLibraryIssue(
                    path: dir.path,
                    message: "비정규 VM 디렉터리 이름입니다. 안전을 위해 불러오지 않았습니다 (정규 이름: \(canonicalID))."
                ))
                continue
            }
            let f = dir.appendingPathComponent("vm.json")
            do {
                let fileValues = try f.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
                guard fileValues.isSymbolicLink != true, fileValues.isRegularFile == true else {
                    issues.append(VMLibraryIssue(
                        path: f.path,
                        message: "VM 설정이 일반 파일이 아니어서 안전을 위해 불러오지 않았습니다."
                    ))
                    continue
                }
                guard let fileSize = fileValues.fileSize, fileSize <= maximumConfigBytes else {
                    issues.append(VMLibraryIssue(
                        path: f.path,
                        message: "VM 설정 파일이 너무 큽니다 (최대 \(maximumConfigBytes)바이트)."
                    ))
                    continue
                }
                let data = try Data(contentsOf: f)
                var cfg = try JSONDecoder().decode(VMConfig.self, from: data)
                // The directory is the authoritative library identity. An
                // embedded ID may be stale, duplicated, or path-like after a
                // manual edit/import; accepting it could target another VM on
                // save/delete.
                cfg.id = canonicalID
                decoded.append((dir, cfg))
            } catch {
                issues.append(VMLibraryIssue(path: f.path, message: "VM 설정을 읽을 수 없습니다: \(error.localizedDescription)"))
            }
        }
        var out: [VMConfig] = []
        for (slug, candidates) in Dictionary(grouping: decoded, by: { $0.config.slug }) {
            guard candidates.count == 1 else {
                for candidate in candidates {
                    issues.append(VMLibraryIssue(
                        path: candidate.directory.path,
                        message: "정규 VM ID '\(slug)'가 다른 항목과 충돌하여 불러오지 않았습니다."
                    ))
                }
                continue
            }
            out.append(candidates[0].config)
        }
        let sorted = out.sorted { $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending }
        return VMLibraryScan(configs: sorted, issues: issues.sorted { $0.path < $1.path })
    }

    static func list(rootURL: URL = root) -> [VMConfig] {
        scan(rootURL: rootURL).configs
    }

    @discardableResult
    static func save(_ config: VMConfig, rootURL: URL = root) -> Bool {
        var cfg = config
        cfg.id = cfg.slug
        let dir = rootURL.appendingPathComponent(cfg.slug, isDirectory: true)
        let configURL = dir.appendingPathComponent("vm.json")
        do {
            if (try? dir.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) == true {
                return false
            }
            if (try? configURL.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) == true {
                return false
            }
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            let enc = JSONEncoder(); enc.outputFormatting = [.prettyPrinted, .sortedKeys]
            let data = try enc.encode(cfg)
            guard data.count <= maximumConfigBytes else { return false }
            try data.write(to: configURL, options: [.atomic])
            return true
        } catch {
            return false
        }
    }

    @discardableResult
    static func delete(_ slug: String, rootURL: URL = root) -> Bool {
        let dir = rootURL.appendingPathComponent(VMConfig.slugify(slug), isDirectory: true)
        do {
            try FileManager.default.removeItem(at: dir)
            return true
        } catch {
            return false
        }
    }

    static func deletionImpact(for config: VMConfig, rootURL: URL = root) -> VMLibraryDeletionImpact {
        let rawEntry = rootURL.appendingPathComponent(config.slug, isDirectory: true)
        if (try? rawEntry.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) == true {
            return .registrationOnly
        }
        let entry = rawEntry
            .resolvingSymlinksInPath()
            .standardizedFileURL
        let bundle = URL(fileURLWithPath: config.bundlePath)
            .resolvingSymlinksInPath()
            .standardizedFileURL
        let entryPrefix = entry.path.hasSuffix("/") ? entry.path : entry.path + "/"
        if bundle.path == entry.path || bundle.path.hasPrefix(entryPrefix) {
            return .managedBundleDeleted
        }
        return .registrationOnly
    }

    /// Move one stopped Windows HVF bundle on the same Mac without changing
    /// its stable VM ID. All in-bundle absolute paths are rebased transactionally
    /// and the unchanged Keychain identity is recorded in a lifecycle receipt.
    static func moveWindowsHVFBundle(
        _ config: VMConfig,
        to destinationParent: URL,
        rootURL: URL = root
    ) -> VMConfig? {
        guard config.engineKind == .hvfEngine else { return nil }
        let fm = FileManager.default
        let source = URL(fileURLWithPath: config.bundlePath, isDirectory: true).standardizedFileURL
        let destination = destinationParent.standardizedFileURL
            .appendingPathComponent(source.lastPathComponent, isDirectory: true)
        guard source != destination,
              (try? source.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey]).isDirectory) == true,
              (try? source.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) != true,
              !fm.fileExists(atPath: destination.path) else { return nil }
        do {
            try fm.createDirectory(at: destinationParent, withIntermediateDirectories: true)
            try fm.moveItem(at: source, to: destination)
        } catch { return nil }

        func rebased(_ path: String) -> String {
            guard !path.isEmpty else { return path }
            if path == source.path { return destination.path }
            let prefix = source.path + "/"
            guard path.hasPrefix(prefix) else { return path }
            return destination.path + "/" + path.dropFirst(prefix.count)
        }

        var moved = config
        moved.bundlePath = destination.path
        moved.runnerPath = rebased(config.runnerPath)
        moved.launchSpecPath = rebased(config.launchSpecPath)
        moved.handoffPath = rebased(config.handoffPath)
        moved.sshKeyPath = rebased(config.sshKeyPath)
        moved.leasesPath = rebased(config.leasesPath)
        moved.isoPath = config.isoPath.map(rebased)
        moved.diskPath = config.diskPath.map(rebased)

        let stateDirectory = destination.appendingPathComponent("metadata/vtpm", isDirectory: true)
        do {
            guard save(moved, rootURL: rootURL) else { throw CocoaError(.fileWriteUnknown) }
            _ = try VTPMIdentityLifecycle(keyStore: KeychainVTPMStateKeyStore())
                .recordSameIdentityMove(
                    stableVMID: moved.slug,
                    stateDirectory: stateDirectory,
                    sourceBundle: source,
                    destinationBundle: destination
                )
            return moved
        } catch {
            _ = save(config, rootURL: rootURL)
            try? fm.moveItem(at: destination, to: source)
            return nil
        }
    }

    /// One-time import of the legacy single ~/.bridgevm-control/config.json into
    /// the library, so the VM the user already runs shows up as the first entry.
    @discardableResult
    static func migrateLegacyIfNeeded() -> Bool {
        migrateLegacyIfNeeded(rootURL: root, legacy: VMConfig.loadLegacy())
    }

    @discardableResult
    static func migrateLegacyIfNeeded(rootURL: URL, legacy: VMConfig?) -> Bool {
        let current = scan(rootURL: rootURL)
        guard current.configs.isEmpty, current.issues.isEmpty, var legacy else { return false }
        if legacy.id == nil { legacy.id = VMConfig.slugify(legacy.name) }
        if legacy.bootMode == nil { legacy.bootMode = "direct-kernel" }
        return save(legacy, rootURL: rootURL)
    }
}
