import Foundation

extension VMLibrary {
    /// Create an install-pending Windows HVF VM; 3D needs signed provenance.
    static func createWindowsHVFInstall(name: String, isoPath: String, diskGiB: Int,
                                        injectViogpu3d: Bool, driverPackageDir: String?,
                                        guestPayloadDirectory: String? = nil,
                                        guestPayloadManifest: String? = nil,
                                        e2eUnattendedPath: String? = nil,
                                        storageDir: URL? = nil,
                                        width: Int = 1280, height: Int = 800,
                                        memMiB: Int = 6144, cpuCount: Int = 4,
                                        networkEnabled: Bool = true,
                                        libraryRoot: URL = root,
                                        persist: Bool = true,
                                        publish: (VMConfig, URL) -> VMRegistrationCommitOutcome = { VMLibrary.saveOutcome($0, rootURL: $1) }) -> HVFCreationOutcome? {
        let fm = FileManager.default
        guard windowsHVFInjectionError(requested: injectViogpu3d) == nil,
              let name = normalizedVMName(name),
              HvfWindowsInstallPlan.diskSizeError(diskGiB) == nil,
              isReadableRegularFile(isoPath),
              let reserved = reserveDestination(
                name, storageBase: storageDir ?? libraryRoot, libraryRoot: libraryRoot
              ) else { return nil }
        let slug = reserved.slug
        let destinationRoot = reserved.root
        var preserveDestination = false
        defer { if !preserveDestination { try? fm.removeItem(at: destinationRoot) } }
        let bundle = destinationRoot.appendingPathComponent("bundle.vmbridge", isDirectory: true)
        do {
            for sub in ["disks", "metadata", "logs/hvf"] {
                try fm.createDirectory(at: bundle.appendingPathComponent(sub), withIntermediateDirectories: true)
            }
        } catch {
            return nil
        }
        let b = bundle.path
        guard let managedISO = WindowsHVFProductPolicy.stageISO(isoPath, in: b) else { return nil }
        guard let inputs = WindowsHVFInstallInputPolicy.stage(
            payloadDirectory: guestPayloadDirectory,
            payloadManifest: guestPayloadManifest,
            e2eUnattendedPath: e2eUnattendedPath, bundlePath: b) else { return nil }
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
        let cfg = VMConfig(id: slug, name: name, displayName: name, backendKind: "hvf-engine",
                           bootMode: "windows-hvf", bundlePath: b, runnerPath: "",
                           launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                           leasesPath: "", guestName: slug,
                           displayWidth: width, displayHeight: height, installPending: true,
                           isoPath: nil, diskPath: "\(b)/disks/hvf-target.raw",
                           memMiB: memMiB, cpuCount: cpuCount, networkEnabled: networkEnabled, experimental3DAllowed: false)
        guard let outcome = HVFCreationOutcome.publish(cfg, libraryRoot: libraryRoot,
            persist: persist, using: publish) else { return nil }
        preserveDestination = true
        return outcome
    }
}
