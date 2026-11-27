import Foundation

struct VMLibraryIssue: Identifiable, Equatable, Sendable {
    let path: String
    let entry: URL
    let message: String
    var id: String { path }

    init(_ entry: URL, file: URL? = nil, message: String) {
        self.entry = entry
        self.path = (file ?? entry).path
        self.message = message
    }
}

struct VMLibraryScan: Sendable {
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
                VMLibraryIssue(rootURL, message: "BridgeVM could not open the library folder. Check its location and permissions in Finder.")
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
                VMLibraryIssue(rootURL, message: "BridgeVM could not list the library folder. Check that the folder is readable in Finder.")
            ])
        }
        var decoded: [(directory: URL, config: VMConfig)] = []
        var issues: [VMLibraryIssue] = []
        for dir in entries {
            let values: URLResourceValues
            do {
                values = try dir.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            } catch {
                issues.append(VMLibraryIssue(dir, message: "BridgeVM could not identify this library entry. Check that it is a readable folder."))
                continue
            }
            if values.isSymbolicLink == true {
                issues.append(VMLibraryIssue(dir, message: "BridgeVM skipped this symbolic link. A VM registration must be stored in a regular folder."))
                continue
            }
            guard values.isDirectory == true, !VMLibraryInternalEntries.isSourceCache(dir) else { continue }
            let canonicalID = VMConfig.slugify(dir.lastPathComponent)
            guard dir.lastPathComponent == canonicalID else {
                issues.append(VMLibraryIssue(
                    dir,
                    message: "BridgeVM skipped this folder because its name does not match the expected VM ID “\(canonicalID)”."
                ))
                continue
            }
            let f = dir.appendingPathComponent("vm.json")
            do {
                let fileValues = try f.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
                guard fileValues.isSymbolicLink != true, fileValues.isRegularFile == true else {
                    issues.append(VMLibraryIssue(
                        dir, file: f,
                        message: "BridgeVM skipped this registration because vm.json must be a regular file."
                    ))
                    continue
                }
                guard let fileSize = fileValues.fileSize, fileSize <= maximumConfigBytes else {
                    issues.append(VMLibraryIssue(
                        dir, file: f,
                        message: "The vm.json file exceeds the limit of \(maximumConfigBytes) bytes."
                    ))
                    continue
                }
                let data = try Data(contentsOf: f)
                var cfg = try JSONDecoder().decode(VMConfig.self, from: data)
                // The directory is authoritative; never trust a persisted path-like ID.
                cfg.id = canonicalID
                let recovery = HvfWindowsInstallFinalization.reconcile(config: cfg, libraryRoot: rootURL)
                cfg = recovery.config
                if let message = recovery.issue { issues.append(VMLibraryIssue(dir, file: f, message: message)) }
                decoded.append((dir, cfg))
            } catch is DecodingError {
                issues.append(VMLibraryIssue(dir, file: f, message: "BridgeVM could not read the settings in vm.json. Check the file or restore it from a backup."))
            } catch {
                issues.append(VMLibraryIssue(dir, file: f, message: "BridgeVM could not open vm.json. Check that the file exists and is readable."))
            }
        }
        var out: [VMConfig] = []
        for (slug, candidates) in Dictionary(grouping: decoded, by: { $0.config.slug }) {
            if candidates.contains(where: { VMRelocationJournal.isPending($0.config, rootURL: rootURL) }) {
                for candidate in candidates {
                    issues.append(VMLibraryIssue(candidate.directory,
                        message: VMRelocationJournal.recoveryIssue(candidate.config, rootURL: rootURL)))
                }
                continue
            }
            guard candidates.count == 1 else {
                for candidate in candidates {
                    issues.append(VMLibraryIssue(
                        candidate.directory,
                        message: "More than one library entry uses the VM ID “\(slug)”. BridgeVM skipped each conflicting entry."
                    ))
                }
                continue
            }
            out.append(candidates[0].config)
        }
        let sorted = out.sorted {
            let order = $0.displayName.localizedCaseInsensitiveCompare($1.displayName)
            return order == .orderedSame ? $0.slug < $1.slug : order == .orderedAscending
        }
        return VMLibraryScan(configs: sorted, issues: issues.sorted { $0.path < $1.path })
    }

    static func list(rootURL: URL = root) -> [VMConfig] {
        scan(rootURL: rootURL).configs
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

    /// Derived from `deletionTarget`, so the dialog's named bundle and this agree.
    static func deletionImpact(for config: VMConfig, rootURL: URL = root) -> VMLibraryDeletionImpact {
        deletionTarget(for: config, rootURL: rootURL) == nil ? .registrationOnly : .managedBundleDeleted
    }

    /// One-time import of the legacy single ~/.bridgevm-control/config.json into
    /// the library, so the VM the user already runs shows up as the first entry.
    @discardableResult
    static func migrateLegacyIfNeeded(rootURL: URL, legacy: VMConfig?) -> Bool {
        let current = scan(rootURL: rootURL)
        guard current.configs.isEmpty, current.issues.isEmpty, var legacy else { return false }
        if legacy.id == nil { legacy.id = VMConfig.slugify(legacy.name) }
        if legacy.bootMode == nil { legacy.bootMode = "direct-kernel" }
        return createRegistration(legacy, rootURL: rootURL)
    }
}

extension VMLibrary {
    /// A handle may rewrite only the unchanged, still-present registration it was
    /// built from; a moved, replaced or removed VM is never reverted or resurrected.
    static func isCurrentRegistration(_ config: VMConfig, rootURL: URL) -> Bool {
        var registered = config
        registered.id = config.slug
        return (try? NativeLibraryReader.readConfig(rootURL: rootURL, id: config.slug)) == registered
    }
}

extension VMLibrary {
    static func recoverWindowsHVFMove(original: VMConfig, moved: VMConfig, pending: URL,
                                     rootURL: URL, fileManager: FileManager) {
        let source = URL(fileURLWithPath: original.bundlePath, isDirectory: true)
        let destination = URL(fileURLWithPath: moved.bundlePath, isDirectory: true)
        try? fileManager.moveItem(at: destination, to: source)
        if let observed = VMRelocationRecovery.configuration(original: original, moved: moved,
                                                             fileManager: fileManager) {
            if save(observed, rootURL: rootURL) {
                try? fileManager.removeItem(at: pending)
            }
        }
    }
}

extension VMLibrary {
    /// New VM publication must not replace a registration created while its
    /// media was being prepared, even when the storage roots are different.
    static func createOutcome(
        _ config: VMConfig, rootURL: URL = root,
        publish: (Data, URL) -> VMRegistrationCommitOutcome = {
            VMRegistrationWriter.commit($0, to: $1, replaceExisting: false)
        }
    ) -> VMRegistrationCommitOutcome {
        saveOutcome(config, rootURL: rootURL, publish: publish)
    }

    static func createRegistration(_ config: VMConfig, rootURL: URL = root,
        preserving preserve: () -> Void = {}) -> Bool {
        let outcome = createOutcome(config, rootURL: rootURL)
        if outcome.preservesPreparedDestination { preserve() }
        return outcome.isCommitted
    }
}

extension VMLibrary {
    /// Deletion recursively removes `<root>/<slug>/`. A stale saved path must not
    /// hide an installer-created `bundle.vmbridge` or first-run imported `bundle`.
    /// External targets of symlinks inside the entry survive recursive deletion.
    static func deletionTarget(for config: VMConfig, rootURL: URL = root) -> URL? {
        let rawEntry = rootURL.appendingPathComponent(config.slug, isDirectory: true)
        if (try? rawEntry.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) == true { return nil }
        let entry = rawEntry.resolvingSymlinksInPath().standardizedFileURL
        let saved = URL(fileURLWithPath: config.bundlePath)
        let bundle = saved.resolvingSymlinksInPath().standardizedFileURL
        let entryPrefix = entry.path.hasSuffix("/") ? entry.path : entry.path + "/"
        if bundle.path == entry.path || bundle.path.hasPrefix(entryPrefix) { return saved }
        return ["bundle.vmbridge", "bundle"].lazy
            .map { rawEntry.appendingPathComponent($0, isDirectory: true) }
            .first {
                let values = try? $0.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
                return values?.isDirectory == true && values?.isSymbolicLink != true
            }
    }
}

extension VMLibrary {
    @discardableResult
    static func save(_ config: VMConfig, rootURL: URL = root) -> Bool {
        saveOutcome(config, rootURL: rootURL).isCommitted
    }

    static func saveOutcome(
        _ config: VMConfig, rootURL: URL = root,
        publish: (Data, URL) -> VMRegistrationCommitOutcome = { VMRegistrationWriter.commit($0, to: $1) }
    ) -> VMRegistrationCommitOutcome {
        var cfg = config
        cfg.id = cfg.slug
        let dir = rootURL.appendingPathComponent(cfg.slug, isDirectory: true)
        let configURL = dir.appendingPathComponent("vm.json")
        do {
            if (try? dir.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) == true
                || (try? configURL.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) == true {
                return .notPublished(CocoaError(.fileWriteInvalidFileName))
            }
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            let data = try encoder.encode(cfg)
            guard data.count <= maximumConfigBytes else {
                return .notPublished(CocoaError(.fileWriteUnknown,
                    userInfo: [NSLocalizedDescriptionKey: "The VM registration exceeds the limit of \(maximumConfigBytes) bytes."]))
            }
            return publish(data, configURL)
        } catch {
            return .notPublished(error)
        }
    }
}

// Failed rollback must not claim the absent original bundle is restored.
extension VMLibrary {
    static func moveWindowsHVFBundle(
        _ config: VMConfig,
        to destinationParent: URL,
        rootURL: URL = root,
        fileManager: FileManager = .default
    ) -> VMConfig? {
        guard VMRelocationReferences.permits(config, rootURL: rootURL) else { return nil }
        let fm = fileManager
        let source = URL(fileURLWithPath: config.bundlePath, isDirectory: true).standardizedFileURL
        let destination = destinationParent.standardizedFileURL
            .appendingPathComponent(source.lastPathComponent, isDirectory: true)
        guard VMRelocationPaths.isSafe(source: source, destination: destination),
              !HvfWindowsInstallTransactionResidue.isPresent(config, rootURL: rootURL),
              (try? source.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey]).isDirectory) == true,
              (try? source.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) != true,
              !fm.fileExists(atPath: destination.path) else { return nil }

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
        moved.diskPath = config.diskPath.map { VMRelocationDiskPath.rebased($0, source: source, destination: destination) ?? rebased($0) }

        let stateDirectory = destination.appendingPathComponent("metadata/vtpm", isDirectory: true)
        do {
            try fm.createDirectory(at: destinationParent, withIntermediateDirectories: true)
        } catch { return nil }
        guard let pending = try? VMRelocationJournal.begin(original: config, moved: moved, rootURL: rootURL) else {
            return nil
        }
        do {
            try fm.moveItem(at: source, to: destination)
        } catch {
            // Nothing moved (a folder BridgeVM can't write to, for one): the bundle is where vm.json
            // says, so the record of an unfinished move goes too. Kept, it hid the VM from the library
            // for good. A move that left anything at the destination keeps it, as before.
            if VMRelocationRecovery.configuration(original: config, moved: moved, fileManager: fm) == config {
                try? fm.removeItem(at: pending)
            }
            return nil
        }
        do {
            guard save(moved, rootURL: rootURL) else { throw CocoaError(.fileWriteUnknown) }
            _ = try VTPMIdentityLifecycle(keyStore: KeychainVTPMStateKeyStore())
                .recordSameIdentityMove(
                    stableVMID: moved.slug,
                    stateDirectory: stateDirectory,
                    sourceBundle: source,
                    destinationBundle: destination
                )
            try fm.removeItem(at: pending)
            return moved
        } catch {
            recoverWindowsHVFMove(original: config, moved: moved, pending: pending,
                                  rootURL: rootURL, fileManager: fm)
            return nil
        }
    }
}
