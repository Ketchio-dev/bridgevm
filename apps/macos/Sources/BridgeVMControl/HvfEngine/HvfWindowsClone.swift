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

        guard archiveCopiedEvidence(in: bundle) else { return nil }

        let cfg = template.windowsCloneRegistration(slug: slug, name: name, bundle: bundle)
        guard createRegistration(cfg, rootURL: libraryRoot, preserving: { succeeded = true }) else { return nil }
        succeeded = true
        return cfg
    }

    /// The copy starts with fresh run evidence; the source's is kept as a source-copy archive. The
    /// copied Windows already has the guest agent, so its `guest-agent-seen` marker stays: without
    /// it the copy's first start attaches the Guest Tools disk again and misreads a slow agent as
    /// missing tools.
    static func archiveCopiedEvidence(in bundle: URL, now: Date = Date()) -> Bool {
        let fm = FileManager.default
        let evidence = bundle.appendingPathComponent("logs/hvf", isDirectory: true)
        var agentSeen = false
        if fm.fileExists(atPath: evidence.path) {
            agentSeen = fm.fileExists(atPath: evidence.appendingPathComponent(agentSeenMarkerName).path)
            let archive = bundle.appendingPathComponent(
                "logs/hvf-source-copy-\(Int(now.timeIntervalSince1970))", isDirectory: true)
            do { try fm.moveItem(at: evidence, to: archive) } catch { return false }
        }
        do { try fm.createDirectory(at: evidence, withIntermediateDirectories: true) } catch { return false }
        if agentSeen { _ = fm.createFile(atPath: evidence.appendingPathComponent(agentSeenMarkerName).path, contents: nil) }
        return true
    }

    static let agentSeenMarkerName = "guest-agent-seen"
}
