import SwiftUI
#if canImport(AppKit)
import AppKit
import UniformTypeIdentifiers
#endif
// MARK: - Create logic (clone Ubuntu / from ISO)

extension VMLibrary {
    static let minimumImportedWindowsHVFDiskGiB: UInt64 = 64
    static let windowsHVFVarsBytes: UInt64 = 64 * 1024 * 1024
    static let maximumVMNameCharacters = 128
    static let maximumVMSlugBytes = 200

    static func normalizedVMName(_ rawName: String) -> String? {
        let name = rawName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty,
              name.count <= maximumVMNameCharacters,
              !name.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains),
              VMConfig.slugify(name).utf8.count <= maximumVMSlugBytes else { return nil }
        return name
    }

    static func windowsHVFImportError(targetDiskPath: String, varsPath: String) -> String? {
        let fm = FileManager.default
        let targetURL = URL(fileURLWithPath: targetDiskPath).resolvingSymlinksInPath().standardizedFileURL
        let varsURL = URL(fileURLWithPath: varsPath).resolvingSymlinksInPath().standardizedFileURL
        guard isReadableRegularFile(targetURL.path) else {
            return "설치된 Windows RAW 디스크 파일을 찾을 수 없습니다."
        }
        guard isReadableRegularFile(varsURL.path) else {
            return "이 VM과 함께 사용한 UEFI vars 파일을 찾을 수 없습니다."
        }
        guard targetURL != varsURL else {
            return "Windows RAW 디스크와 UEFI vars는 서로 다른 파일이어야 합니다."
        }
        let targetBytes = ((try? fm.attributesOfItem(atPath: targetURL.path)[.size] as? NSNumber)?.uint64Value) ?? 0
        guard targetBytes > 0 else { return "Windows RAW 디스크가 비어 있습니다." }
        let varsBytes = ((try? fm.attributesOfItem(atPath: varsURL.path)[.size] as? NSNumber)?.uint64Value) ?? 0
        guard varsBytes == windowsHVFVarsBytes else {
            return "UEFI vars는 정확히 64 MiB여야 합니다 (현재 \(varsBytes)바이트)."
        }
        if let handle = try? FileHandle(forReadingFrom: targetURL) {
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
        _ base: String,
        storageBase: URL,
        libraryRoot: URL = root
    ) -> (slug: String, root: URL)? {
        let baseSlug = VMConfig.slugify(base)
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

    static func cloneOrCopyFile(from source: String, to destination: String) -> Bool {
        let fm = FileManager.default
        let clone = Shell.run("/bin/cp", ["-c", source, destination])
        if clone.code == 0 { return true }
        try? fm.removeItem(atPath: destination)
        do {
            try fm.copyItem(atPath: source, toPath: destination)
            return true
        } catch {
            return false
        }
    }

    private static func cloneOrCopyDirectory(from source: String, to destination: String) -> Bool {
        let fm = FileManager.default
        let clone = Shell.run("/bin/cp", ["-c", "-R", source, destination])
        if clone.code == 0 { return true }
        try? fm.removeItem(atPath: destination)
        do {
            try fm.copyItem(atPath: source, toPath: destination)
            return true
        } catch {
            return false
        }
    }

    private static func isReadableRegularFile(_ path: String) -> Bool {
        let url = URL(fileURLWithPath: path).resolvingSymlinksInPath().standardizedFileURL
        guard (try? url.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true else {
            return false
        }
        return FileManager.default.isReadableFile(atPath: url.path)
    }

    private static func isReadableDirectory(_ path: String) -> Bool {
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

    static func rewriteCloneMetadata(
        at path: String,
        oldBundlePath: String,
        newBundlePath: String,
        newName: String
    ) -> Bool {
        guard var rootObject = JSONFile.loadDict(path) else { return false }

        func rewrite(_ value: Any) -> Any {
            if let string = value as? String {
                return string.replacingOccurrences(of: oldBundlePath, with: newBundlePath)
            }
            if let array = value as? [Any] { return array.map(rewrite) }
            if let dictionary = value as? [String: Any] {
                return dictionary.mapValues(rewrite)
            }
            return value
        }

        rootObject = rewrite(rootObject) as? [String: Any] ?? rootObject
        rootObject["vm_name"] = newName
        return JSONFile.writeDict(rootObject, to: path)
    }

    /// Create a brand-new Linux VM that boots an arbitrary ISO installer via EFI.
    /// The blank target disk is where the user installs the distro.
    static func createFromISO(name: String, isoPath: String, template: VMConfig,
                              storageDir: URL? = nil, width: Int = 1440, height: Int = 900,
                              diskGiB: Int = 40, memMiB: Int = 4096, cpuCount: Int = 4) -> VMConfig? {
        let fm = FileManager.default
        guard let name = normalizedVMName(name),
              diskGiB > 0, width > 0, height > 0,
              isReadableRegularFile(isoPath),
              let reserved = reserveDestination(name, storageBase: storageDir ?? root) else { return nil }
        let slug = reserved.slug
        let destinationRoot = reserved.root
        var succeeded = false
        defer {
            if !succeeded { try? fm.removeItem(at: destinationRoot) }
        }
        let bundle = destinationRoot.appendingPathComponent("bundle.vmbridge", isDirectory: true)
        do {
            for sub in ["disks", "metadata", "logs", "nvram"] {
                try fm.createDirectory(at: bundle.appendingPathComponent(sub), withIntermediateDirectories: true)
            }
        } catch {
            return nil
        }
        let b = bundle.path
        let diskPath = "\(b)/disks/root.raw"
        let isoLocal = "\(b)/disks/installer.iso"
        // blank install target (sparse)
        guard Shell.run("/usr/bin/truncate", ["-s", "\(diskGiB)G", diskPath]).code == 0 else { return nil }
        // reference the user's ISO via a clone (instant on APFS) so it is stable
        guard cloneOrCopyFile(from: isoPath, to: isoLocal) else { return nil }

        let launchSpecPath = "\(b)/metadata/apple-vz-launch.json"
        let handoffPath = "\(b)/metadata/handoff.json"
        let serialLog = "\(b)/logs/serial.log"

        let resources: [String: Any] = ["memory": String(memMiB), "cpu": String(cpuCount), "display_fps_cap": "adaptive", "rationale": "New ISO VM", "balloon_device": true]
        let disk: [String: Any] = ["path": diskPath, "format": "raw", "read_only": false]
        let guest: [String: Any] = ["os": "linux", "arch": "arm64"]
        let isoDict: [String: Any] = ["path": isoLocal, "exists": true]
        let boot: [String: Any] = ["mode": "iso-efi", "iso": isoDict, "efi_var_store": "\(b)/nvram/efivars.bin"]
        let devices: [String: Any] = ["entropy_device": true, "network": "nat", "serial_log_path": serialLog]
        let integration: [String: Any] = ["clipboard": true, "dynamic_resolution": true, "shared_folders": true, "virtiofs": true]
        let readiness: [String: Any] = ["ready": true, "blockers": []]
        // Build dicts by subscript assignment to avoid Swift type-check timeout on big literals.
        var launchSpec: [String: Any] = [:]
        launchSpec["vm_name"] = name; launchSpec["bundle_path"] = b
        launchSpec["guest"] = guest; launchSpec["boot"] = boot
        launchSpec["disk"] = disk; launchSpec["resources"] = resources
        launchSpec["devices"] = devices; launchSpec["integration"] = integration
        launchSpec["logs"] = ["runner_log_path": "\(b)/logs/lightvm.log"]
        launchSpec["readiness"] = readiness
        guard JSONFile.writeDict(launchSpec, to: launchSpecPath) else { return nil }
        var handoff: [String: Any] = [:]
        handoff["backend"] = "apple-virtualization-framework"; handoff["vm_name"] = name
        handoff["bundle_path"] = b; handoff["launch_spec_path"] = launchSpecPath
        handoff["guest"] = guest; handoff["boot_mode"] = "iso-efi"
        handoff["disk"] = disk; handoff["resources"] = resources
        handoff["runner_log_path"] = "\(b)/logs/lightvm.log"
        handoff["serial_log_path"] = serialLog; handoff["integration"] = integration
        handoff["readiness"] = readiness
        guard JSONFile.writeDict(handoff, to: handoffPath) else { return nil }

        let cfg = VMConfig(id: slug, name: name, displayName: name, backendKind: "fast-vz",
                           bootMode: "iso-efi", bundlePath: b, runnerPath: template.runnerPath,
                           launchSpecPath: launchSpecPath, handoffPath: handoffPath,
                           sshKeyPath: template.sshKeyPath, sshUser: "user", leasesPath: template.leasesPath,
                           guestName: slug, displayWidth: width, displayHeight: height,
                           installPending: true, memMiB: memMiB, cpuCount: cpuCount)
        guard save(cfg) else { return nil }
        succeeded = true
        return cfg
    }

    /// Rewrite the Fast VZ launch-spec/handoff `resources.memory` and
    /// `resources.cpu` of a bundle to the requested allocation. Returns false
    /// on any read/write failure so the caller can abort the create.
    static func applyResourceOverride(bundlePath b: String, memMiB: Int, cpuCount: Int) -> Bool {
        for file in ["\(b)/metadata/apple-vz-launch.json", "\(b)/metadata/handoff.json"] {
            guard var root = JSONFile.loadDict(file) else { continue }
            var resources = (root["resources"] as? [String: Any]) ?? [:]
            resources["memory"] = String(memMiB)
            resources["cpu"] = String(cpuCount)
            root["resources"] = resources
            guard JSONFile.writeDict(root, to: file) else { return false }
        }
        return true
    }

    /// Duplicate the default Ubuntu VM (instant APFS clone of the bundle + fresh
    /// machine identity), so a new ready-to-run Ubuntu lands in the library.
    static func cloneUbuntu(name: String, template: VMConfig, storageDir: URL? = nil,
                            width: Int = 1440, height: Int = 900,
                            memMiB: Int? = nil, cpuCount: Int? = nil) -> VMConfig? {
        let fm = FileManager.default
        let sourceBundle = URL(fileURLWithPath: template.bundlePath, isDirectory: true)
        let storageBase = storageDir ?? root
        guard let name = normalizedVMName(name),
              width > 0, height > 0,
              isReadableDirectory(template.bundlePath),
              !isSameOrDescendant(storageBase, of: sourceBundle),
              let reserved = reserveDestination(name, storageBase: storageBase) else { return nil }
        let slug = reserved.slug
        let destDir = reserved.root
        var succeeded = false
        defer { if !succeeded { try? fm.removeItem(at: destDir) } }
        let bundle = destDir.appendingPathComponent("bundle.vmbridge", isDirectory: true).path
        // APFS copy-on-write clone: instant, no extra disk for the 14G rootfs.
        guard cloneOrCopyDirectory(from: template.bundlePath, to: bundle) else { return nil }
        let b = bundle
        // fresh identity so two VMs don't collide on MAC/machine-id
        for identity in ["\(b)/metadata/machine-identifier.bin", "\(b)/metadata/network-mac-address.txt"] {
            if fm.fileExists(atPath: identity) {
                do { try fm.removeItem(atPath: identity) } catch { return nil }
            }
        }
        // rewrite absolute paths inside launch-spec + handoff from old bundle -> new
        for f in ["\(b)/metadata/apple-vz-launch.json", "\(b)/metadata/handoff.json"] {
            guard rewriteCloneMetadata(
                at: f,
                oldBundlePath: template.bundlePath,
                newBundlePath: b,
                newName: name
            ) else { return nil }
        }
        if let memMiB, let cpuCount {
            guard applyResourceOverride(bundlePath: b, memMiB: memMiB, cpuCount: cpuCount) else { return nil }
        }
        var cfg = template
        cfg.id = slug
        cfg.name = name
        cfg.displayName = name
        cfg.bundlePath = b
        cfg.launchSpecPath = "\(b)/metadata/apple-vz-launch.json"
        cfg.handoffPath = "\(b)/metadata/handoff.json"
        cfg.bootMode = "direct-kernel"
        cfg.installPending = false
        cfg.displayWidth = width
        cfg.displayHeight = height
        if let memMiB { cfg.memMiB = memMiB }
        if let cpuCount { cfg.cpuCount = cpuCount }
        guard save(cfg) else { return nil }
        succeeded = true
        return cfg
    }

    /// APFS-clone an installed Windows HVF bundle while deliberately starting
    /// the copy with a distinct TPM identity. The copied encrypted TPM state is
    /// retained inside the new bundle as a source-copy archive and receipted.
    static func cloneWindowsHVF(
        name: String,
        template: VMConfig,
        storageDir: URL? = nil,
        libraryRoot: URL = root,
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
              !isSameOrDescendant(storageBase, of: sourceBundle),
              let reserved = reserveDestination(
                  name,
                  storageBase: storageBase,
                  libraryRoot: libraryRoot
              ) else { return nil }
        let slug = reserved.slug
        let destinationRoot = reserved.root
        var succeeded = false
        defer { if !succeeded { try? fm.removeItem(at: destinationRoot) } }
        let bundle = destinationRoot.appendingPathComponent("bundle.vmbridge", isDirectory: true)
        guard cloneOrCopyDirectory(from: template.bundlePath, to: bundle.path) else { return nil }
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
        guard save(cfg, rootURL: libraryRoot) else { return nil }
        succeeded = true
        return cfg
    }

    /// Create a Windows 11 ARM VM (QEMU + HVF + swtpm + edk2). Blank qcow2 install
    /// target; the Win11 ISO boots its installer in a cocoa window.
    static func createWindows(name: String, isoPath: String, template: VMConfig,
                              storageDir: URL? = nil, width: Int = 1280, height: Int = 800,
                              diskGiB: Int = 64, persist: Bool = true,
                              memMiB: Int = 6144, cpuCount: Int = 4,
                              diskCreator: ((String, Int) -> Bool)? = nil) -> VMConfig? {
        let fm = FileManager.default
        guard let name = normalizedVMName(name),
              diskGiB > 0, width > 0, height > 0,
              isReadableRegularFile(isoPath),
              let reserved = reserveDestination(name, storageBase: storageDir ?? root) else { return nil }
        let slug = reserved.slug
        let destinationRoot = reserved.root
        let bundle = destinationRoot.appendingPathComponent("bundle.vmbridge", isDirectory: true)
        var succeeded = false
        defer { if !succeeded { try? fm.removeItem(at: destinationRoot) } }
        do {
            for sub in ["disks", "metadata", "logs"] {
                try fm.createDirectory(at: bundle.appendingPathComponent(sub), withIntermediateDirectories: true)
            }
        } catch {
            return nil
        }
        let b = bundle.path
        let disk = "\(b)/disks/win.qcow2"
        let isoLocal = "\(b)/disks/installer.iso"
        let sourceISO = URL(fileURLWithPath: isoPath).resolvingSymlinksInPath().standardizedFileURL.path
        guard cloneOrCopyFile(from: sourceISO, to: isoLocal) else { return nil }
        let createdDisk = diskCreator?(disk, diskGiB) ?? {
            Shell.run("/opt/homebrew/bin/qemu-img", ["create", "-f", "qcow2", disk, "\(diskGiB)G"]).code == 0
        }()
        guard createdDisk, fm.fileExists(atPath: disk) else { return nil }
        let cfg = VMConfig(id: slug, name: name, displayName: name, backendKind: "qemu-compat",
                           bootMode: "windows-iso", bundlePath: b, runnerPath: "",
                           launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                           leasesPath: template.leasesPath, guestName: slug,
                           displayWidth: width, displayHeight: height, installPending: true,
                           isoPath: isoLocal, diskPath: disk, memMiB: memMiB, cpuCount: cpuCount)
        if persist, !save(cfg) { return nil }
        succeeded = true
        return cfg
    }

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
                                        persist: Bool = true) -> VMConfig? {
        let fm = FileManager.default
        guard windowsHVFInjectionError(requested: injectViogpu3d) == nil,
              let name = normalizedVMName(name),
              diskGiB >= Int(HvfWindowsInstallPlan.minimumDiskGiB),
              isReadableRegularFile(isoPath),
              let reserved = reserveDestination(
                name, storageBase: storageDir ?? libraryRoot, libraryRoot: libraryRoot
              ) else { return nil }
        let slug = reserved.slug
        let destinationRoot = reserved.root
        var succeeded = false
        defer { if !succeeded { try? fm.removeItem(at: destinationRoot) } }
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
        if persist, !save(cfg, rootURL: libraryRoot) { return nil }
        succeeded = true
        return cfg
    }

    static func windowsHVFInjectionError(requested: Bool) -> String? {
        guard requested else { return nil }
        return HvfWindowsDriverPreflight.message(for: HvfWindowsDriverPreflight.provenanceBlocker)
    }


}
