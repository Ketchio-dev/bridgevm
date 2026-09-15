import Foundation

extension VMLibrary {
    /// Import an installed raw disk and matching writable UEFI vars.
    static func createWindowsHVF(name: String, targetDiskPath: String, varsPath: String,
                                 storageDir: URL? = nil, width: Int = 1280, height: Int = 800,
                                 memMiB: Int = 6144, cpuCount: Int = 4,
                                 networkEnabled: Bool = true, injectViogpu3d: Bool = false,
                                 libraryRoot: URL = root,
                                 persist: Bool = true, snapshotHelper: URL = HvfMediaImportHelper.bundled) -> VMConfig? {
        let fm = FileManager.default
        guard windowsHVFInjectionError(requested: injectViogpu3d) == nil,
              let name = normalizedVMName(name),
              windowsHVFImportError(targetDiskPath: targetDiskPath, varsPath: varsPath) == nil else { return nil }

        let storageBase = storageDir ?? libraryRoot
        guard let reserved = reserveDestination(
            name, storageBase: storageBase, libraryRoot: libraryRoot
        ) else { return nil }
        let slug = reserved.slug
        let destinationRoot = reserved.root
        var succeeded = false
        defer { if !succeeded { try? fm.removeItem(at: destinationRoot) } }
        let bundle = destinationRoot.appendingPathComponent("bundle.vmbridge", isDirectory: true)
        let disk = bundle.appendingPathComponent("disks/hvf-target.raw").path
        let vars = bundle.appendingPathComponent("metadata/hvf-vars.fd").path
        let sourceDisk = URL(fileURLWithPath: targetDiskPath).resolvingSymlinksInPath().standardizedFileURL
        let sourceVars = URL(fileURLWithPath: varsPath).resolvingSymlinksInPath().standardizedFileURL
        guard sourceDisk.path != URL(fileURLWithPath: disk).standardizedFileURL.path,
              sourceVars.path != URL(fileURLWithPath: vars).standardizedFileURL.path else { return nil }
        do {
            for sub in ["disks", "metadata", "logs/hvf"] {
                try fm.createDirectory(at: bundle.appendingPathComponent(sub), withIntermediateDirectories: true)
            }
        } catch {
            return nil
        }
        do { try HvfMediaImport.copy(disk: sourceDisk.path, vars: sourceVars.path,
            toDisk: URL(fileURLWithPath: disk), toVars: URL(fileURLWithPath: vars), helper: snapshotHelper)
        } catch { return nil }
        let minimumDiskBytes = minimumImportedWindowsHVFDiskGiB * 1024 * 1024 * 1024
        let importedDiskSize = ((try? fm.attributesOfItem(atPath: disk)[.size] as? NSNumber)?.uint64Value) ?? 0
        guard growSparseFileIfNeeded(at: disk, minimumBytes: minimumDiskBytes) else { return nil }
        if importedDiskSize < minimumDiskBytes {
            let marker = bundle.appendingPathComponent("metadata/hvf-grow-pending")
            guard fm.createFile(atPath: marker.path, contents: Data("\(minimumDiskBytes)\n".utf8)) else { return nil }
        }
        guard fm.createFile(atPath: bundle.appendingPathComponent("metadata/hvf.ctl").path, contents: nil) else { return nil }

        let b = bundle.path
        let cfg = VMConfig(id: slug, name: name, displayName: name, backendKind: "hvf-engine",
                           bootMode: "windows-hvf", bundlePath: b, runnerPath: "",
                           launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                           leasesPath: "", guestName: slug,
                           displayWidth: width, displayHeight: height, installPending: false,
                           isoPath: nil, diskPath: disk, memMiB: memMiB, cpuCount: cpuCount,
                           networkEnabled: networkEnabled, experimental3DAllowed: false)
        if persist, !save(cfg, rootURL: libraryRoot) { return nil }
        succeeded = true
        return cfg
    }

    private static func growSparseFileIfNeeded(at path: String, minimumBytes: UInt64) -> Bool {
        guard let size = (try? FileManager.default.attributesOfItem(atPath: path)[.size] as? NSNumber)?.uint64Value else {
            return false
        }
        guard size < minimumBytes else { return true }
        guard let handle = try? FileHandle(forWritingTo: URL(fileURLWithPath: path)) else { return false }
        defer { try? handle.close() }
        do {
            try handle.truncate(atOffset: minimumBytes)
            return true
        } catch {
            return false
        }
    }

}
