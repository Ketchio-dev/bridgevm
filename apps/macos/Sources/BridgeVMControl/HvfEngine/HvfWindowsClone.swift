import Foundation

extension VMLibrary {
    /// APFS-clone an installed Windows HVF bundle while deliberately starting
    /// the copy with a distinct TPM identity. The copied encrypted TPM state is
    /// retained inside the new bundle as a source-copy archive and receipted.
    static func cloneWindowsHVF(
        name: String,
        template: VMConfig,
        storageDir: URL? = nil,
        libraryRoot: URL = root,
        beforeCopy: () -> Bool = { true },
        afterCopy: () -> Bool = { true }
    ) -> VMConfig? {
        let fm = FileManager.default
        let sourceBundle = URL(fileURLWithPath: template.bundlePath, isDirectory: true)
        let storageBase = storageDir ?? libraryRoot
        guard template.engineKind == .hvfEngine,
              template.installPending != true,
              !VMRelocationJournal.isPending(template, rootURL: libraryRoot),
              let name = normalizedVMName(name),
              isReadableDirectory(template.bundlePath),
              HvfWindowsCloneSource.refusal(for: template, phase: .copy) == nil,
              VMRelocationPaths.isSafe(source: sourceBundle, destination: storageBase),
              let reserved = reserveDestination(
                  name,
                  storageBase: storageBase,
                  libraryRoot: libraryRoot
              ) else { return nil }
        let slug = reserved.slug
        let destinationRoot = reserved.root
        var succeeded = false
        defer { if !succeeded { VMCreationDestinationCleanup.removeIfUnpublished(destinationRoot, libraryRoot: libraryRoot) } }
        let bundle = destinationRoot.appendingPathComponent("bundle.vmbridge", isDirectory: true)
        guard beforeCopy(), cloneOrCopyDirectory(from: template.bundlePath, to: bundle.path) else { return nil }
        guard afterCopy() else { return nil }

        let lifecycle = VTPMIdentityLifecycle(keyStore: KeychainVTPMStateKeyStore())
        let copiedState = bundle.appendingPathComponent("metadata/vtpm", isDirectory: true)
        do {
            _ = try lifecycle.prepareClonedIdentity(
                newStableVMID: slug,
                copiedStateDirectory: copiedState
            )
        } catch { return nil }

        let copiedEvidence = bundle.appendingPathComponent("logs/hvf", isDirectory: true)
        if fm.fileExists(atPath: copiedEvidence.path) {
            let archive = bundle.appendingPathComponent(
                "logs/hvf-source-copy-\(Int(Date().timeIntervalSince1970))",
                isDirectory: true
            )
            do { try fm.moveItem(at: copiedEvidence, to: archive) } catch { return nil }
        }
        do {
            try fm.createDirectory(at: copiedEvidence, withIntermediateDirectories: true)
        } catch { return nil }

        var cfg = template
        cfg.id = slug
        cfg.name = name
        cfg.displayName = name
        cfg.bundlePath = bundle.path
        cfg.diskPath = bundle.appendingPathComponent("disks/hvf-target.raw").path
        cfg.guestName = slug
        cfg.installPending = false
        guard createRegistration(cfg, rootURL: libraryRoot, preserving: { succeeded = true }) else { return nil }
        succeeded = true
        return cfg
    }
}
