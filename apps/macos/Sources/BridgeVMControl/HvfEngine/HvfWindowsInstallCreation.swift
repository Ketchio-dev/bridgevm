import Foundation

/// What a failed Create says when the user can do something about it.
enum CreateFailure {
    static let isoUnreadable = "BridgeVM can’t read the Windows ISO. Reconnect its drive, or choose the ISO again."
    static let isoCopy = "BridgeVM couldn’t copy the Windows ISO into the VM’s folder. Check that both drives are still connected and that there’s enough free space."

    static func folder(_ base: URL) -> String {
        "BridgeVM couldn’t make the VM’s folder in “\(base.path)”. Check that the drive is connected and that you can save files there."
    }

    /// A copy to another volume needs the ISO's size free there, plus 1 GiB; a clone on the same volume
    /// needs none.
    static func isoSpace(iso: String, destination: URL) -> String? {
        var source = stat(), folder = stat(), target = statfs()
        guard stat(iso, &source) == 0, stat(destination.path, &folder) == 0,
              statfs(destination.path, &target) == 0 else { return nil }
        return isoSpace(isoBytes: UInt64(max(source.st_size, 0)), sameVolume: source.st_dev == folder.st_dev,
                        freeBytes: UInt64(target.f_bavail) * UInt64(target.f_bsize),
                        volume: (try? destination.resourceValues(forKeys: [.volumeNameKey]).volumeName) ?? destination.path)
    }

    static func isoSpace(isoBytes: UInt64, sameVolume: Bool, freeBytes: UInt64, volume: String) -> String? {
        guard !sameVolume, freeBytes < isoBytes + (1 << 30) else { return nil }
        let needed = (isoBytes + (1 << 30) + (1 << 30) - 1) >> 30
        return "“\(volume)” doesn’t have enough free space to copy the Windows ISO (about \(needed) GB). Free up space, or choose another location."
    }
}

extension VMLibrary {
    /// Create an install-pending Windows HVF VM; 3D needs signed provenance.
    static func createWindowsHVFInstall(name: String, isoPath: String, diskGiB: Int,
                                        injectViogpu3d: Bool, driverPackageDir: String?,
                                        guestPayloadDirectory: String? = nil,
                                        guestPayloadManifest: String? = nil,
                                        e2eUnattendedPath: String? = nil,
                                        storageDir: URL? = nil,
                                        width: Int = 1280, height: Int = 800,
                                        guestResolution: String? = nil,
                                        memMiB: Int = 6144, cpuCount: Int = 4,
                                        networkEnabled: Bool = true,
                                        libraryRoot: URL = root,
                                        repoRoot: URL = HvfEngineSession.defaultRepoRoot(),
                                        persist: Bool = true,
                                        publish: (VMConfig, URL) -> VMRegistrationCommitOutcome = { VMLibrary.createOutcome($0, rootURL: $1) }) -> HVFCreationOutcome? {
        let fm = FileManager.default
        // The user's own driver folder: say what is wrong with it before the ISO is copied, not the
        // general sentence below, nor a setup failure at Start that blames the ISO.
        if guestPayloadDirectory != nil || guestPayloadManifest != nil,
           let problem = HvfWindowsGuestPayloadIdentity.inspect(payloadDirectory: guestPayloadDirectory,
                                                                 manifestPath: guestPayloadManifest).error
               ?? HvfWindowsGuestPayloadIdentity.manifestFormatError(guestPayloadManifest) {
            return .refused(problem)
        }
        guard windowsHVFInjectionError(requested: injectViogpu3d) == nil,
              let name = normalizedVMName(name),
              HvfWindowsInstallPlan.diskSizeError(diskGiB) == nil else { return nil }
        // Each failure the user can act on says what to do, not the general sentence.
        guard isReadableRegularFile(isoPath) else { return .refused(CreateFailure.isoUnreadable) }
        let storageBase = storageDir ?? libraryRoot
        guard let reserved = reserveDestination(name, storageBase: storageBase, libraryRoot: libraryRoot) else {
            return .refused(CreateFailure.folder(storageBase))
        }
        let slug = reserved.slug
        let destinationRoot = reserved.root
        var preserveDestination = false
        defer { if !preserveDestination { VMCreationDestinationCleanup.removeIfUnpublished(destinationRoot, libraryRoot: libraryRoot) } }
        let bundle = destinationRoot.appendingPathComponent("bundle.vmbridge", isDirectory: true)
        do {
            for sub in ["disks", "metadata", "logs/hvf"] {
                try fm.createDirectory(at: bundle.appendingPathComponent(sub), withIntermediateDirectories: true)
            }
        } catch {
            return .refused(CreateFailure.folder(storageBase))
        }
        let b = bundle.path
        if let space = CreateFailure.isoSpace(iso: isoPath, destination: bundle) { return .refused(space) }
        guard let managedISO = WindowsHVFProductPolicy.stageISO(isoPath, in: b) else {
            return .refused(CreateFailure.isoCopy)
        }
        guard let inputs = WindowsHVFInstallInputPolicy.stage(
            payloadDirectory: guestPayloadDirectory,
            payloadManifest: guestPayloadManifest,
            e2eUnattendedPath: e2eUnattendedPath, bundlePath: b,
            repoRoot: repoRoot) else { return nil }
        let request = HvfWindowsInstallRequest(
            isoPath: managedISO.path, isoSHA256: managedISO.sha256,
            diskGiB: diskGiB,
            injectViogpu3d: injectViogpu3d,
            driverPackageDir: driverPackageDir,
            guestPayloadDirectory: inputs.payload?.directory,
            guestPayloadManifest: inputs.payload?.manifest,
            guestPayloadIdentity: inputs.payload?.identity,
            unattendedPath: inputs.unattended?.path,
            unattendedIdentity: inputs.unattended?.sha256
        )
        guard request.save(bundlePath: b) else { return nil }
        var cfg = VMConfig(id: slug, name: name, displayName: name, backendKind: "hvf-engine",
                           bootMode: "windows-hvf", bundlePath: b, runnerPath: "",
                           launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                           leasesPath: "", guestName: slug,
                           displayWidth: width, displayHeight: height, installPending: true,
                           isoPath: nil, diskPath: "\(b)/disks/hvf-target.raw",
                           memMiB: memMiB, cpuCount: cpuCount, networkEnabled: networkEnabled, experimental3DAllowed: false)
        cfg.guestResolution = guestResolution
        VMLibrary.windowsShareRegistrationLock.lock()
        defer { VMLibrary.windowsShareRegistrationLock.unlock() }
        cfg.enableNewWindowsShare(otherRegistrations: VMLibrary.scan(rootURL: libraryRoot).configs)
        guard let outcome = HVFCreationOutcome.publish(cfg, libraryRoot: libraryRoot,
            persist: persist, preserving: { preserveDestination = true }, using: publish) else { return nil }
        preserveDestination = true
        return outcome
    }
}
