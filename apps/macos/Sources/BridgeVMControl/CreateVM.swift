import SwiftUI
#if canImport(AppKit)
import AppKit
import UniformTypeIdentifiers
#endif

// MARK: - Create logic (clone Ubuntu / from ISO)

extension VMLibrary {
    static let minimumImportedWindowsHVFDiskGiB: UInt64 = 64
    static let windowsHVFVarsBytes: UInt64 = 64 * 1024 * 1024

    static func windowsHVFImportError(targetDiskPath: String, varsPath: String) -> String? {
        let fm = FileManager.default
        var targetIsDirectory: ObjCBool = false
        var varsIsDirectory: ObjCBool = false
        guard fm.fileExists(atPath: targetDiskPath, isDirectory: &targetIsDirectory), !targetIsDirectory.boolValue else {
            return "설치된 Windows RAW 디스크 파일을 찾을 수 없습니다."
        }
        guard fm.fileExists(atPath: varsPath, isDirectory: &varsIsDirectory), !varsIsDirectory.boolValue else {
            return "이 VM과 함께 사용한 UEFI vars 파일을 찾을 수 없습니다."
        }
        let targetBytes = ((try? fm.attributesOfItem(atPath: targetDiskPath)[.size] as? NSNumber)?.uint64Value) ?? 0
        guard targetBytes > 0 else { return "Windows RAW 디스크가 비어 있습니다." }
        let varsBytes = ((try? fm.attributesOfItem(atPath: varsPath)[.size] as? NSNumber)?.uint64Value) ?? 0
        guard varsBytes == windowsHVFVarsBytes else {
            return "UEFI vars는 정확히 64 MiB여야 합니다 (현재 \(varsBytes)바이트)."
        }
        if let handle = try? FileHandle(forReadingFrom: URL(fileURLWithPath: targetDiskPath)) {
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

    private static func uniqueSlug(_ base: String) -> String {
        let baseSlug = VMConfig.slugify(base)
        let existing = Set(list().map { $0.slug })
        var slug = baseSlug; var n = 2
        while existing.contains(slug) { slug = "\(baseSlug)-\(n)"; n += 1 }
        return slug
    }

    private static func cloneOrCopyFile(from source: String, to destination: String) -> Bool {
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

    /// Create a brand-new Linux VM that boots an arbitrary ISO installer via EFI.
    /// The blank target disk is where the user installs the distro.
    static func createFromISO(name: String, isoPath: String, template: VMConfig,
                              storageDir: URL? = nil, width: Int = 1440, height: Int = 900,
                              diskGiB: Int = 40) -> VMConfig? {
        let slug = uniqueSlug(name)
        let bundle = (storageDir ?? root).appendingPathComponent(slug, isDirectory: true).appendingPathComponent("bundle.vmbridge", isDirectory: true)
        let fm = FileManager.default
        for sub in ["disks", "metadata", "logs", "nvram"] {
            try? fm.createDirectory(at: bundle.appendingPathComponent(sub), withIntermediateDirectories: true)
        }
        let b = bundle.path
        let diskPath = "\(b)/disks/root.raw"
        let isoLocal = "\(b)/disks/installer.iso"
        // blank install target (sparse)
        Shell.run("/usr/bin/truncate", ["-s", "\(diskGiB)G", diskPath])
        // reference the user's ISO via a clone (instant on APFS) so it is stable
        Shell.run("/bin/cp", ["-c", isoPath, isoLocal])

        let launchSpecPath = "\(b)/metadata/apple-vz-launch.json"
        let handoffPath = "\(b)/metadata/handoff.json"
        let serialLog = "\(b)/logs/serial.log"

        let resources: [String: Any] = ["memory": "4096", "cpu": "4", "display_fps_cap": "adaptive", "rationale": "New ISO VM", "balloon_device": true]
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
        JSONFile.writeDict(launchSpec, to: launchSpecPath)
        var handoff: [String: Any] = [:]
        handoff["backend"] = "apple-virtualization-framework"; handoff["vm_name"] = name
        handoff["bundle_path"] = b; handoff["launch_spec_path"] = launchSpecPath
        handoff["guest"] = guest; handoff["boot_mode"] = "iso-efi"
        handoff["disk"] = disk; handoff["resources"] = resources
        handoff["runner_log_path"] = "\(b)/logs/lightvm.log"
        handoff["serial_log_path"] = serialLog; handoff["integration"] = integration
        handoff["readiness"] = readiness
        JSONFile.writeDict(handoff, to: handoffPath)

        let cfg = VMConfig(id: slug, name: name, displayName: name, backendKind: "fast-vz",
                           bootMode: "iso-efi", bundlePath: b, runnerPath: template.runnerPath,
                           launchSpecPath: launchSpecPath, handoffPath: handoffPath,
                           sshKeyPath: template.sshKeyPath, sshUser: "user", leasesPath: template.leasesPath,
                           guestName: slug, displayWidth: width, displayHeight: height,
                           installPending: true)
        guard save(cfg) else {
            try? fm.removeItem(at: bundle.deletingLastPathComponent())
            return nil
        }
        return cfg
    }

    /// Duplicate the default Ubuntu VM (instant APFS clone of the bundle + fresh
    /// machine identity), so a new ready-to-run Ubuntu lands in the library.
    static func cloneUbuntu(name: String, template: VMConfig, storageDir: URL? = nil, width: Int = 1440, height: Int = 900) -> VMConfig? {
        let slug = uniqueSlug(name)
        let destDir = (storageDir ?? root).appendingPathComponent(slug, isDirectory: true)
        try? FileManager.default.createDirectory(at: destDir, withIntermediateDirectories: true)
        let bundle = destDir.appendingPathComponent("bundle.vmbridge", isDirectory: true).path
        // APFS copy-on-write clone: instant, no extra disk for the 14G rootfs.
        let r = Shell.run("/bin/cp", ["-c", "-R", template.bundlePath, bundle])
        if r.code != 0 { return nil }
        let b = bundle
        // fresh identity so two VMs don't collide on MAC/machine-id
        try? FileManager.default.removeItem(atPath: "\(b)/metadata/machine-identifier.bin")
        try? FileManager.default.removeItem(atPath: "\(b)/metadata/network-mac-address.txt")
        // rewrite absolute paths inside launch-spec + handoff from old bundle -> new
        for f in ["\(b)/metadata/apple-vz-launch.json", "\(b)/metadata/handoff.json"] {
            if let data = FileManager.default.contents(atPath: f),
               var s = String(data: data, encoding: .utf8) {
                s = s.replacingOccurrences(of: template.bundlePath, with: b)
                s = s.replacingOccurrences(of: "\"vm_name\" : \"\(template.name)\"", with: "\"vm_name\" : \"\(name)\"")
                s = s.replacingOccurrences(of: "\"vm_name\": \"\(template.name)\"", with: "\"vm_name\": \"\(name)\"")
                try? s.write(toFile: f, atomically: true, encoding: .utf8)
            }
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
        guard save(cfg) else {
            try? FileManager.default.removeItem(at: destDir)
            return nil
        }
        return cfg
    }

    /// Create a Windows 11 ARM VM (QEMU + HVF + swtpm + edk2). Blank qcow2 install
    /// target; the Win11 ISO boots its installer in a cocoa window.
    static func createWindows(name: String, isoPath: String, template: VMConfig,
                              storageDir: URL? = nil, width: Int = 1280, height: Int = 800,
                              diskGiB: Int = 64) -> VMConfig? {
        let slug = uniqueSlug(name)
        let bundle = (storageDir ?? root).appendingPathComponent(slug, isDirectory: true).appendingPathComponent("bundle.vmbridge", isDirectory: true)
        for sub in ["disks", "metadata", "logs"] {
            try? FileManager.default.createDirectory(at: bundle.appendingPathComponent(sub), withIntermediateDirectories: true)
        }
        let b = bundle.path
        let disk = "\(b)/disks/win.qcow2"
        let r = Shell.run("/opt/homebrew/bin/qemu-img", ["create", "-f", "qcow2", disk, "\(diskGiB)G"])
        if r.code != 0 { return nil }
        let cfg = VMConfig(id: slug, name: name, displayName: name, backendKind: "qemu-compat",
                           bootMode: "windows-iso", bundlePath: b, runnerPath: "",
                           launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                           leasesPath: template.leasesPath, guestName: slug,
                           displayWidth: width, displayHeight: height, installPending: true,
                           isoPath: isoPath, diskPath: disk, memMiB: 6144, cpuCount: 4)
        guard save(cfg) else {
            try? FileManager.default.removeItem(at: bundle.deletingLastPathComponent())
            return nil
        }
        return cfg
    }

    /// Import a proven installed Windows raw disk and its matching writable UEFI
    /// vars store. A blank raw disk is not selectable because this engine does
    /// not implement the Windows installer path yet.
    static func createWindowsHVF(name: String, targetDiskPath: String, varsPath: String,
                                 storageDir: URL? = nil, width: Int = 1280, height: Int = 800,
                                 persist: Bool = true) -> VMConfig? {
        let fm = FileManager.default
        guard windowsHVFImportError(targetDiskPath: targetDiskPath, varsPath: varsPath) == nil else { return nil }

        let storageBase = storageDir ?? root
        let baseSlug = uniqueSlug(name)
        var slug = baseSlug
        var suffix = 2
        while fm.fileExists(atPath: storageBase.appendingPathComponent(slug, isDirectory: true).path) {
            slug = "\(baseSlug)-\(suffix)"
            suffix += 1
        }
        let destinationRoot = storageBase.appendingPathComponent(slug, isDirectory: true)
        let bundle = destinationRoot.appendingPathComponent("bundle.vmbridge", isDirectory: true)
        let disk = bundle.appendingPathComponent("disks/hvf-target.raw").path
        let vars = bundle.appendingPathComponent("metadata/hvf-vars.fd").path
        let sourceDisk = URL(fileURLWithPath: targetDiskPath).resolvingSymlinksInPath().standardizedFileURL
        let sourceVars = URL(fileURLWithPath: varsPath).resolvingSymlinksInPath().standardizedFileURL
        guard sourceDisk.path != URL(fileURLWithPath: disk).standardizedFileURL.path,
              sourceVars.path != URL(fileURLWithPath: vars).standardizedFileURL.path else { return nil }

        var succeeded = false
        defer {
            if !succeeded { try? fm.removeItem(at: destinationRoot) }
        }
        do {
            for sub in ["disks", "metadata", "logs/hvf"] {
                try fm.createDirectory(at: bundle.appendingPathComponent(sub), withIntermediateDirectories: true)
            }
        } catch {
            return nil
        }
        guard cloneOrCopyFile(from: sourceDisk.path, to: disk),
              cloneOrCopyFile(from: sourceVars.path, to: vars) else { return nil }
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
                           isoPath: nil, diskPath: disk, memMiB: 6144, cpuCount: 4)
        if persist, !save(cfg) { return nil }
        succeeded = true
        return cfg
    }
}

// MARK: - Create sheet (gallery)

struct CreateVMSheet: View {
    @ObservedObject var library: LibraryModel
    @Environment(\.dismiss) private var dismiss

    @State private var mode: Mode = .ubuntu
    @State private var name = ""
    @State private var isoPath: String = ""
    @State private var hvfTargetPath: String = ""
    @State private var hvfVarsPath: String = ""
    @State private var storageDir: URL? = nil
    @State private var resIndex = 1
    @State private var working = false
    @State private var error = ""

    private let resolutions = [(1280, 800), (1440, 900), (1920, 1080), (2560, 1440)]
    enum Mode { case ubuntu, iso, windows, windowsHVF }

    private var template: VMConfig? {
        library.vms.first { $0.backendKind == "fast-vz" && ($0.bootMode ?? "direct-kernel") == "direct-kernel" }
            ?? library.vms.first
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("새 VM 만들기").font(.title2.bold())

            HStack(spacing: 12) {
                tile("Ubuntu", "checkmark.seal.fill", selected: mode == .ubuntu) { mode = .ubuntu }
                tile("Linux ISO", "opticaldisc", selected: mode == .iso) { mode = .iso }
                tile("Windows QEMU", "macwindow", selected: mode == .windows) { mode = .windows; autofillWin11() }
                tile("Windows HVF", "cpu", selected: mode == .windowsHVF) { mode = .windowsHVF; isoPath = "" }
            }

            if mode == .ubuntu {
                Text("기본 Ubuntu 데스크톱을 즉시 복제합니다 (APFS 클론, 추가 용량 없음).")
                    .font(.callout).foregroundColor(.secondary)
            } else if mode == .windowsHVF {
                Text("이미 설치되어 정상 부팅한 Windows ARM RAW 디스크와 그 부팅에 사용한 UEFI vars를 가져옵니다. 원본은 변경하지 않고 라이브러리에 복제합니다.")
                    .font(.callout).foregroundColor(.secondary)
                HStack {
                    Button("설치된 RAW 선택…") { pickHVFTarget() }
                    Text(hvfTargetPath.isEmpty ? "선택된 디스크 없음" : (hvfTargetPath as NSString).lastPathComponent)
                        .font(.caption).foregroundColor(.secondary).lineLimit(1)
                }
                HStack {
                    Button("UEFI vars 선택…") { pickHVFVars() }
                    Text(hvfVarsPath.isEmpty ? "선택된 vars 없음" : (hvfVarsPath as NSString).lastPathComponent)
                        .font(.caption).foregroundColor(.secondary).lineLimit(1)
                }
            } else {
                Text(mode == .windows
                     ? "Windows 11 ARM ISO를 선택하면 QEMU + TPM 2.0으로 설치 마법사를 부팅합니다."
                     : "원하는 리눅스 배포판 ISO를 선택하면 EFI로 부팅해 설치하는 새 VM을 만듭니다.")
                    .font(.callout).foregroundColor(.secondary)
                HStack {
                    Button("ISO 선택…") { pickISO() }
                    Text(isoPath.isEmpty ? "선택된 ISO 없음" : (isoPath as NSString).lastPathComponent)
                        .font(.caption).foregroundColor(.secondary).lineLimit(1)
                }
            }

            HStack {
                Text("이름").frame(width: 64, alignment: .leading)
                TextField(mode == .ubuntu ? "Ubuntu 2" : "Fedora 40", text: $name).textFieldStyle(.roundedBorder)
            }

            HStack {
                Text("저장 위치").frame(width: 64, alignment: .leading)
                Button("폴더 선택…") { pickStorage() }
                Text(storageDir?.path ?? "기본 (라이브러리)")
                    .font(.caption).foregroundColor(.secondary).lineLimit(1).truncationMode(.middle)
                if storageDir != nil { Button { storageDir = nil } label: { Image(systemName: "xmark.circle.fill") }.buttonStyle(.plain) }
            }

            HStack {
                Text("해상도").frame(width: 64, alignment: .leading)
                Picker("", selection: $resIndex) {
                    ForEach(0..<resolutions.count, id: \.self) { i in
                        Text("\(resolutions[i].0)×\(resolutions[i].1)").tag(i)
                    }
                }.labelsHidden().frame(width: 150)
                Spacer()
            }

            if !error.isEmpty { Text(error).font(.caption).foregroundColor(.red) }

            HStack {
                Spacer()
                Button("취소") { dismiss() }
                Button(working ? "생성 중…" : "생성") { create() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(!canCreate)
            }
        }
        .padding(20)
        .frame(width: 480)
    }

    private func tile(_ title: String, _ icon: String, selected: Bool, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            VStack(spacing: 8) {
                Image(systemName: icon).font(.system(size: 28))
                Text(title).font(.callout)
            }
            .frame(maxWidth: .infinity).padding(.vertical, 16)
            .background(selected ? Color.accentColor.opacity(0.18) : Color.gray.opacity(0.1))
            .overlay(RoundedRectangle(cornerRadius: 10).stroke(selected ? Color.accentColor : .clear, lineWidth: 2))
            .cornerRadius(10)
        }.buttonStyle(.plain)
    }

    private func pickISO() {
        if let url = chooseFile(directories: false, extensions: ["iso", "img", "dmg"]) { isoPath = url.path }
    }

    private func pickStorage() {
        if let url = chooseFile(directories: true) { storageDir = url }
    }

    private func pickHVFTarget() {
        if let url = chooseFile(directories: false, extensions: ["raw", "img"]) { hvfTargetPath = url.path }
    }

    private func pickHVFVars() {
        if let url = chooseFile(directories: false, extensions: ["fd", "vars"]) { hvfVarsPath = url.path }
    }

    private var canCreate: Bool {
        guard !working, !name.trimmingCharacters(in: .whitespaces).isEmpty else { return false }
        switch mode {
        case .windowsHVF:
            return !hvfTargetPath.isEmpty && !hvfVarsPath.isEmpty
        case .iso, .windows:
            return !isoPath.isEmpty && template != nil
        case .ubuntu:
            return template != nil
        }
    }

    private func chooseFile(directories: Bool, extensions: [String]? = nil) -> URL? {
        #if canImport(AppKit)
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = directories
        panel.canChooseFiles = !directories
        if let extensions { panel.allowedContentTypes = extensions.compactMap { UTType(filenameExtension: $0) } }
        return panel.runModal() == .OK ? panel.url : nil
        #else
        return nil
        #endif
    }

    private func autofillWin11() {
        guard isoPath.isEmpty else { return }
        let candidates = [
            "\(NSHomeDirectory())/Downloads/Win11_25H2_English_Arm64_v2.iso",
            "/Users/user/Desktop/Projects/Working/Virtual Computer/ISO/Win11_25H2_English_Arm64_v2.iso",
        ]
        for p in candidates where FileManager.default.fileExists(atPath: p) { isoPath = p; return }
    }

    private func create() {
        let selectedTemplate = template
        if mode != .windowsHVF, selectedTemplate == nil { error = "템플릿 VM이 없습니다"; return }
        working = true; error = ""
        let nm = name.trimmingCharacters(in: .whitespaces)
        let m = mode; let iso = isoPath
        let hvfTarget = hvfTargetPath; let hvfVars = hvfVarsPath
        if mode == .windowsHVF,
           let importError = VMLibrary.windowsHVFImportError(targetDiskPath: hvfTarget, varsPath: hvfVars) {
            error = importError
            working = false
            return
        }
        let sd = storageDir; let w = resolutions[resIndex].0; let h = resolutions[resIndex].1
        Task.detached {
            let cfg: VMConfig?
            switch m {
            case .ubuntu:
                cfg = selectedTemplate.flatMap { VMLibrary.cloneUbuntu(name: nm, template: $0, storageDir: sd, width: w, height: h) }
            case .iso:
                cfg = selectedTemplate.flatMap { VMLibrary.createFromISO(name: nm, isoPath: iso, template: $0, storageDir: sd, width: w, height: h) }
            case .windows:
                cfg = selectedTemplate.flatMap { VMLibrary.createWindows(name: nm, isoPath: iso, template: $0, storageDir: sd, width: w, height: h) }
            case .windowsHVF:
                cfg = VMLibrary.createWindowsHVF(name: nm, targetDiskPath: hvfTarget, varsPath: hvfVars,
                                                 storageDir: sd, width: w, height: h)
            }
            await MainActor.run {
                working = false
                if let cfg = cfg, library.add(cfg) { dismiss() }
                else { error = "생성 또는 VM 라이브러리 저장 실패" }
            }
        }
    }
}
