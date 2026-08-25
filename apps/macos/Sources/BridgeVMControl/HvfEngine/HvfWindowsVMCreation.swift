import Foundation

extension VMLibrary {
    static func windowsHVFInjectionError(
        requested: Bool,
        packageDirectory: String? = nil,
        now: Date = Date(),
        trustAnchors: [HvfWindowsKernelPolicyVerifier.TrustAnchor] =
            HvfWindowsKernelPolicyVerifier.productionTrustAnchors
    ) -> String? {
        guard requested else { return nil }
        guard let packageDirectory, !packageDirectory.isEmpty else {
            return HvfWindowsDriverPreflight.message(
                for: HvfWindowsDriverPreflight.provenanceBlocker)
        }
        return HvfWindowsDriverPreflight.inspect(
            packageDirectory: packageDirectory, now: now,
            trustAnchors: trustAnchors).userMessage
    }

    private static func prepareInjectionSnapshot(
        requested: Bool,
        packageDirectory: String?,
        now: Date,
        trustAnchors: [HvfWindowsKernelPolicyVerifier.TrustAnchor]
    ) -> HvfWindowsPreparedPackageSnapshot? {
        guard requested, let packageDirectory, !packageDirectory.isEmpty else { return nil }
        let source = URL(fileURLWithPath: packageDirectory, isDirectory: true)
        guard case .success(let prepared) = HvfWindowsPreparedPackageSnapshot.prepare(
            source: source, now: now, trustAnchors: trustAnchors) else { return nil }
        return prepared
    }

    /// Create an install-pending Windows HVF VM. An injection request stores
    /// only a verified bundle-private snapshot, never the selected source path.
    static func createWindowsHVFInstall(
        name: String, isoPath: String, diskGiB: Int,
        injectViogpu3d: Bool, driverPackageDir: String?,
        storageDir: URL? = nil, width: Int = 1280, height: Int = 800,
        memMiB: Int = 6144, cpuCount: Int = 4, networkEnabled: Bool = true,
        persist: Bool = true, verificationDate: Date = Date(),
        trustAnchors: [HvfWindowsKernelPolicyVerifier.TrustAnchor] =
            HvfWindowsKernelPolicyVerifier.productionTrustAnchors
    ) -> VMConfig? {
        let prepared = prepareInjectionSnapshot(
            requested: injectViogpu3d, packageDirectory: driverPackageDir,
            now: verificationDate, trustAnchors: trustAnchors)
        guard !injectViogpu3d || prepared != nil,
              let name = normalizedVMName(name),
              diskGiB >= Int(HvfWindowsInstallPlan.minimumDiskGiB),
              isReadableRegularFile(isoPath),
              let reserved = reserveDestination(name, storageBase: storageDir ?? root) else {
            return nil
        }
        let fm = FileManager.default
        let destinationRoot = reserved.root
        var succeeded = false
        defer { if !succeeded { try? fm.removeItem(at: destinationRoot) } }
        let bundle = destinationRoot.appendingPathComponent("bundle.vmbridge", isDirectory: true)
        do {
            for sub in ["disks", "metadata", "logs/hvf"] {
                try fm.createDirectory(
                    at: bundle.appendingPathComponent(sub), withIntermediateDirectories: true)
            }
        } catch { return nil }
        if let prepared,
           case .failure = prepared.publish(bundle: bundle) { return nil }
        let b = bundle.path
        let request = HvfWindowsInstallRequest(
            isoPath: URL(fileURLWithPath: isoPath).resolvingSymlinksInPath().standardizedFileURL.path,
            diskGiB: diskGiB, injectViogpu3d: injectViogpu3d,
            driverPackageDir: injectViogpu3d
                ? HvfWindowsPreparedPackageSnapshot.bundleRelativePath : nil,
            importedMedia: nil)
        guard request.save(bundlePath: b) else { return nil }
        let cfg = VMConfig(
            id: reserved.slug, name: name, displayName: name, backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: b, runnerPath: "", launchSpecPath: "",
            handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
            guestName: reserved.slug, displayWidth: width, displayHeight: height,
            installPending: true, isoPath: nil, diskPath: "\(b)/disks/hvf-target.raw",
            memMiB: memMiB, cpuCount: cpuCount, networkEnabled: networkEnabled)
        if persist, !save(cfg) { return nil }
        succeeded = true
        return cfg
    }

    /// Import an installed raw disk and matching writable UEFI vars. Optional
    /// injection is performed later against temporary clones, never these
    /// newly imported canonical bundle files.
    static func createWindowsHVF(
        name: String, targetDiskPath: String, varsPath: String,
        storageDir: URL? = nil, width: Int = 1280, height: Int = 800,
        memMiB: Int = 6144, cpuCount: Int = 4, networkEnabled: Bool = true,
        injectViogpu3d: Bool = false, injectionISOPath: String? = nil,
        driverPackageDir: String? = nil, persist: Bool = true,
        verificationDate: Date = Date(),
        trustAnchors: [HvfWindowsKernelPolicyVerifier.TrustAnchor] =
            HvfWindowsKernelPolicyVerifier.productionTrustAnchors
    ) -> VMConfig? {
        let prepared = prepareInjectionSnapshot(
            requested: injectViogpu3d, packageDirectory: driverPackageDir,
            now: verificationDate, trustAnchors: trustAnchors)
        guard !injectViogpu3d || prepared != nil,
              !injectViogpu3d || injectionISOPath.map(isReadableRegularFile) == true,
              let name = normalizedVMName(name),
              windowsHVFImportError(
                targetDiskPath: targetDiskPath, varsPath: varsPath) == nil else { return nil }
        let fm = FileManager.default
        guard let reserved = reserveDestination(
            name, storageBase: storageDir ?? root) else { return nil }
        let destinationRoot = reserved.root
        var succeeded = false
        defer { if !succeeded { try? fm.removeItem(at: destinationRoot) } }
        let bundle = destinationRoot.appendingPathComponent("bundle.vmbridge", isDirectory: true)
        let disk = bundle.appendingPathComponent("disks/hvf-target.raw").path
        let vars = bundle.appendingPathComponent("metadata/hvf-vars.fd").path
        let sourceDisk = URL(fileURLWithPath: targetDiskPath)
            .resolvingSymlinksInPath().standardizedFileURL
        let sourceVars = URL(fileURLWithPath: varsPath)
            .resolvingSymlinksInPath().standardizedFileURL
        guard sourceDisk.path != URL(fileURLWithPath: disk).standardizedFileURL.path,
              sourceVars.path != URL(fileURLWithPath: vars).standardizedFileURL.path else { return nil }
        do {
            for sub in ["disks", "metadata", "logs/hvf"] {
                try fm.createDirectory(
                    at: bundle.appendingPathComponent(sub), withIntermediateDirectories: true)
            }
        } catch { return nil }
        if let prepared,
           case .failure = prepared.publish(bundle: bundle) { return nil }
        guard cloneOrCopyFile(from: sourceDisk.path, to: disk),
              cloneOrCopyFile(from: sourceVars.path, to: vars) else { return nil }
        let minimumBytes = minimumImportedWindowsHVFDiskGiB * 1024 * 1024 * 1024
        let importedSize = ((try? fm.attributesOfItem(
            atPath: disk)[.size] as? NSNumber)?.uint64Value) ?? 0
        guard growSparseFileIfNeeded(at: disk, minimumBytes: minimumBytes) else { return nil }
        if importedSize < minimumBytes {
            let marker = bundle.appendingPathComponent("metadata/hvf-grow-pending")
            guard fm.createFile(
                atPath: marker.path, contents: Data("\(minimumBytes)\n".utf8)) else { return nil }
        }
        guard fm.createFile(
            atPath: bundle.appendingPathComponent("metadata/hvf.ctl").path,
            contents: nil) else { return nil }
        let b = bundle.path
        if injectViogpu3d {
            let request = HvfWindowsInstallRequest(
                isoPath: URL(fileURLWithPath: injectionISOPath!)
                    .resolvingSymlinksInPath().standardizedFileURL.path,
                diskGiB: Int(minimumImportedWindowsHVFDiskGiB),
                injectViogpu3d: true,
                driverPackageDir: HvfWindowsPreparedPackageSnapshot.bundleRelativePath,
                importedMedia: true)
            guard request.save(bundlePath: b) else { return nil }
        }
        let cfg = VMConfig(
            id: reserved.slug, name: name, displayName: name, backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: b, runnerPath: "", launchSpecPath: "",
            handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
            guestName: reserved.slug, displayWidth: width, displayHeight: height,
            installPending: injectViogpu3d, isoPath: nil, diskPath: disk,
            memMiB: memMiB, cpuCount: cpuCount, networkEnabled: networkEnabled)
        if persist, !save(cfg) { return nil }
        succeeded = true
        return cfg
    }
}
