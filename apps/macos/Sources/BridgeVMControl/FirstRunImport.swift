import Foundation

/// D2 first-run import: register an existing HVF Windows VM bundle (an installed
/// raw disk + 64 MiB UEFI vars + optional vTPM state dir) into the library so a
/// cold GUI start can boot it. Import-only: no ISO install, no from-scratch VM
/// creation — those stay separate flows. Validation is fail-closed at the
/// system boundary (user-picked paths), mirroring VTPMStateSecurity's stance.
enum FirstRunImport {
    /// The three inputs a user selects. Only the disk and vars are required; a
    /// vTPM state dir is optional (a fresh TPM is initialized when absent).
    struct Inputs: Equatable {
        var displayName: String
        var diskPath: String
        var varsPath: String
        var vtpmStateDir: String?
        var memMiB: Int
        var cpuCount: Int
    }

    enum ValidationError: Error, Equatable, CustomStringConvertible {
        case emptyName
        case diskMissing(String)
        case diskNotAFile(String)
        case diskEmpty(String)
        case varsMissing(String)
        case varsWrongSize(path: String, bytes: UInt64)
        case vtpmNotADirectory(String)
        case badResources(memMiB: Int, cpuCount: Int)

        var description: String {
            switch self {
            case .emptyName: return "VM 이름을 입력하세요."
            case .diskMissing(let p): return "디스크 이미지를 찾을 수 없습니다: \(p)"
            case .diskNotAFile(let p): return "디스크 경로가 파일이 아닙니다: \(p)"
            case .diskEmpty(let p): return "디스크 이미지가 비어 있습니다: \(p)"
            case .varsMissing(let p): return "UEFI vars 파일을 찾을 수 없습니다: \(p)"
            case .varsWrongSize(let p, let b):
                return "UEFI vars 파일은 정확히 64 MiB여야 합니다 (\(p): \(b) bytes)."
            case .vtpmNotADirectory(let p): return "vTPM 상태 경로가 디렉터리가 아닙니다: \(p)"
            case .badResources(let mem, let cpu):
                return "RAM/CPU 값이 유효하지 않습니다 (RAM \(mem) MiB, CPU \(cpu))."
            }
        }
    }

    static let requiredVarsBytes: UInt64 = 64 * 1024 * 1024

    /// Validate user inputs against the filesystem. Pure w.r.t. the injected
    /// FileManager so it is unit-testable without touching real user paths.
    static func validate(
        _ inputs: Inputs,
        fileManager: FileManager = .default
    ) -> ValidationError? {
        if inputs.displayName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return .emptyName
        }
        if inputs.memMiB < 2048 || inputs.cpuCount < 1 || inputs.cpuCount > 123 {
            return .badResources(memMiB: inputs.memMiB, cpuCount: inputs.cpuCount)
        }

        var isDir: ObjCBool = false
        guard fileManager.fileExists(atPath: inputs.diskPath, isDirectory: &isDir) else {
            return .diskMissing(inputs.diskPath)
        }
        if isDir.boolValue { return .diskNotAFile(inputs.diskPath) }
        let diskBytes = (try? fileManager.attributesOfItem(atPath: inputs.diskPath)[.size]
            as? NSNumber)?.uint64Value ?? 0
        if diskBytes == 0 { return .diskEmpty(inputs.diskPath) }

        var varsIsDir: ObjCBool = false
        guard fileManager.fileExists(atPath: inputs.varsPath, isDirectory: &varsIsDir),
            !varsIsDir.boolValue
        else {
            return .varsMissing(inputs.varsPath)
        }
        let varsBytes = (try? fileManager.attributesOfItem(atPath: inputs.varsPath)[.size]
            as? NSNumber)?.uint64Value ?? 0
        if varsBytes != requiredVarsBytes {
            return .varsWrongSize(path: inputs.varsPath, bytes: varsBytes)
        }

        if let vtpm = inputs.vtpmStateDir, !vtpm.isEmpty {
            var vtpmIsDir: ObjCBool = false
            guard fileManager.fileExists(atPath: vtpm, isDirectory: &vtpmIsDir),
                vtpmIsDir.boolValue
            else {
                return .vtpmNotADirectory(vtpm)
            }
        }
        return nil
    }

    /// Bundle layout the HVF backend expects (see HvfEngineConfig).
    struct BundleLayout {
        let bundleURL: URL
        var diskURL: URL { bundleURL.appendingPathComponent("disks/hvf-target.raw") }
        var varsURL: URL { bundleURL.appendingPathComponent("metadata/hvf-vars.fd") }
        var vtpmURL: URL { bundleURL.appendingPathComponent("metadata/vtpm", isDirectory: true) }
    }

    /// Materialize the bundle for `slug` under `libraryRoot` and place the
    /// selected inputs into it. The disk is hard-linked when possible (same
    /// volume) to avoid copying tens of GiB, falling back to a copy across
    /// volumes. Returns the persisted VMConfig.
    static func register(
        _ inputs: Inputs,
        slug: String,
        libraryRoot: URL,
        fileManager: FileManager = .default
    ) throws -> VMConfig {
        let bundleURL = libraryRoot
            .appendingPathComponent(slug, isDirectory: true)
            .appendingPathComponent("bundle", isDirectory: true)
        let layout = BundleLayout(bundleURL: bundleURL)
        try fileManager.createDirectory(
            at: layout.diskURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try fileManager.createDirectory(
            at: layout.varsURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try fileManager.createDirectory(at: layout.vtpmURL, withIntermediateDirectories: true)

        try placeLarge(from: inputs.diskPath, to: layout.diskURL, fileManager: fileManager)
        if fileManager.fileExists(atPath: layout.varsURL.path) {
            try fileManager.removeItem(at: layout.varsURL)
        }
        try fileManager.copyItem(atPath: inputs.varsPath, toPath: layout.varsURL.path)
        if let vtpm = inputs.vtpmStateDir, !vtpm.isEmpty {
            let contents = (try? fileManager.contentsOfDirectory(atPath: vtpm)) ?? []
            for entry in contents where entry != ".lock" {
                let src = (vtpm as NSString).appendingPathComponent(entry)
                let dst = layout.vtpmURL.appendingPathComponent(entry)
                if fileManager.fileExists(atPath: dst.path) {
                    try fileManager.removeItem(at: dst)
                }
                try fileManager.copyItem(atPath: src, toPath: dst.path)
            }
        }

        var config = VMConfig(
            id: slug,
            name: inputs.displayName,
            displayName: inputs.displayName,
            backendKind: BackendKind.hvfEngine.rawValue,
            bundlePath: bundleURL.path,
            runnerPath: "",
            launchSpecPath: "",
            handoffPath: "",
            sshKeyPath: "",
            sshUser: "bridge",
            leasesPath: "",
            guestName: inputs.displayName,
            displayWidth: 1280,
            displayHeight: 720
        )
        config.diskPath = layout.diskURL.path
        config.memMiB = inputs.memMiB
        config.cpuCount = inputs.cpuCount
        return config
    }

    /// Hard-link a large file when on the same volume; copy otherwise. Any
    /// existing destination is replaced.
    private static func placeLarge(
        from sourcePath: String,
        to destURL: URL,
        fileManager: FileManager
    ) throws {
        if fileManager.fileExists(atPath: destURL.path) {
            try fileManager.removeItem(at: destURL)
        }
        do {
            try fileManager.linkItem(atPath: sourcePath, toPath: destURL.path)
        } catch {
            try fileManager.copyItem(atPath: sourcePath, toPath: destURL.path)
        }
    }
}
