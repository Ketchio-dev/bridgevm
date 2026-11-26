import Foundation

/// D2 first-run import: register an existing HVF Windows VM bundle (an installed
/// disk + optional 64 MiB UEFI vars + optional vTPM state dir) into the library so a
/// cold GUI start can boot it. Import-only: no ISO install, no from-scratch VM
/// creation — those stay separate flows. Validation is fail-closed at the
/// system boundary (user-picked paths), mirroring VTPMStateSecurity's stance.
enum FirstRunImport {
    /// The three inputs a user selects. Only the disk is required; a
    /// vTPM state dir is optional (a fresh TPM is initialized when absent).
    struct Inputs: Equatable, Sendable {
        var displayName: String
        var diskPath: String
        var varsPath: String = ""
        var vtpmStateDir: String?; var vtpmRecoveryPackagePath: String? = nil
        var vtpmRecoveryCodePath: String? = nil
        var memMiB: Int
        var cpuCount: Int
    }

    enum ValidationError: LocalizedError, Equatable, CustomStringConvertible {
        case emptyName
        case diskMissing(String)
        case diskNotAFile(String)
        case diskEmpty(String)
        case unsupportedDiskFormat(String)
        case varsMissing(String)
        case varsWrongSize(path: String, bytes: UInt64)
        case vtpmNotADirectory(String), vtpmRecoveryMissing(String), vtpmRecoveryWithoutState
        case vtpmRecoveryPackageRequired, vtpmRecoveryCodeRequired
        case badResources(memMiB: Int, cpuCount: Int)

        var errorDescription: String? { description }

        var description: String {
            switch self {
            case .emptyName: return "VM 이름을 입력하세요."
            case .diskMissing(let p): return "디스크 이미지를 찾을 수 없습니다: \(p)"
            case .diskNotAFile(let p): return "디스크 경로가 파일이 아닙니다: \(p)"
            case .diskEmpty(let p): return "디스크 이미지가 비어 있습니다: \(p)"
            case .unsupportedDiskFormat(let format):
                return "\(format) 디스크는 직접 가져올 수 없으니 RAW, QCOW2 또는 VHDX로 변환한 뒤 다시 선택하세요."
            case .varsMissing(let p): return "UEFI vars 파일을 찾을 수 없습니다: \(p)"
            case .varsWrongSize(let p, let b):
                return "UEFI vars 파일은 정확히 64 MiB여야 합니다 (\(p): \(b) bytes)."
            case .vtpmNotADirectory(let p): return "vTPM 상태 경로가 디렉터리가 아닙니다: \(p)"
            case .vtpmRecoveryMissing(let p): return "vTPM 복구 파일을 찾을 수 없습니다: \(p)"
            case .vtpmRecoveryPackageRequired: return "vTPM 복구 패키지를 선택하세요."
            case .vtpmRecoveryCodeRequired: return "vTPM 복구 코드 파일을 선택하세요."
            case .vtpmRecoveryWithoutState: return "vTPM 복구 파일을 사용하려면 상태 폴더도 선택하세요."
            case .badResources(let mem, let cpu):
                return "RAM은 2048 MiB 이상, CPU는 1~64개여야 합니다 (RAM \(mem) MiB, CPU \(cpu))."
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
        if inputs.memMiB < 2048 || !HvfEngineConfig.supportedCPURange.contains(inputs.cpuCount) {
            return .badResources(memMiB: inputs.memMiB, cpuCount: inputs.cpuCount)
        }

        var isDir: ObjCBool = false
        guard fileManager.fileExists(atPath: inputs.diskPath, isDirectory: &isDir) else {
            return .diskMissing(inputs.diskPath)
        }
        if isDir.boolValue { return .diskNotAFile(inputs.diskPath) }
        let diskBytes = (try? fileManager.attributesOfItem(atPath:
            URL(fileURLWithPath: inputs.diskPath).resolvingSymlinksInPath().path)[.size] as? NSNumber)?.uint64Value ?? 0
        if diskBytes == 0 { return .diskEmpty(inputs.diskPath) }
        if let error = HvfImportedDiskFormat.detect(path: inputs.diskPath)?.importError { return error }

        if !inputs.varsPath.isEmpty {
            var varsIsDir: ObjCBool = false
            guard fileManager.fileExists(atPath: inputs.varsPath, isDirectory: &varsIsDir),
                !varsIsDir.boolValue
            else {
                return .varsMissing(inputs.varsPath)
            }
            let varsBytes = (try? fileManager.attributesOfItem(atPath:
                URL(fileURLWithPath: inputs.varsPath).resolvingSymlinksInPath().path)[.size] as? NSNumber)?.uint64Value ?? 0
            if varsBytes != requiredVarsBytes {
                return .varsWrongSize(path: inputs.varsPath, bytes: varsBytes)
            }
        }
        if let error = FirstRunImportVTPMValidation.validate(inputs, fileManager: fileManager) { return error }
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
    /// selected inputs into it. Disk and vars are independent copies so guest
    /// writes cannot modify the source or another imported VM; a container
    /// disk becomes a RAW copy.
    /// Returns the configuration for the caller to persist.
    static func register(
        _ inputs: Inputs,
        slug: String,
        libraryRoot: URL,
        fileManager: FileManager = .default, snapshotHelper: URL = HvfMediaImportHelper.bundled
    ) throws -> VMConfig {
        let prepared = try prepare(inputs, slug: slug, libraryRoot: libraryRoot,
            fileManager: fileManager, snapshotHelper: snapshotHelper)
        VMLibrary.windowsShareRegistrationLock.withLock { prepared.assignNewShare(in: libraryRoot) }
        prepared.preserve()
        return prepared.config
    }
}

/// Owns the reserved entry until publication or a conflict requires preservation.
/// This object stays on the import worker; only the value configuration crosses actors.
final class FirstRunImportPreparation {
    private(set) var config: VMConfig
    private let destination: FirstRunImportDestination
    private let importedVTPMKey: ImportedVTPMKeyCustody?
    private var preserved = false

    init(config: VMConfig, destination: FirstRunImportDestination, importedVTPMKey: ImportedVTPMKeyCustody?) {
        self.config = config
        self.destination = destination
        self.importedVTPMKey = importedVTPMKey
    }

    func assignNewShare(in libraryRoot: URL) {
        config.enableNewWindowsShare(otherRegistrations: VMLibrary.scan(rootURL: libraryRoot).configs)
    }

    func preserve() { preserved = true; importedVTPMKey?.preserve() }
    deinit {
        if !preserved, !destination.removeIfOwned() { importedVTPMKey?.preserve() }
    }
}

extension FirstRunImport {
    static func prepare(
        _ inputs: Inputs, slug: String, libraryRoot: URL,
        fileManager: FileManager = .default, snapshotHelper: URL = HvfMediaImportHelper.bundled,
        converting: @escaping (HvfImportedDiskFormat, Int) -> Void = { _, _ in }
    ) throws -> FirstRunImportPreparation {
        let destination = try FirstRunImportDestination(
            slug: slug, libraryRoot: libraryRoot, fileManager: fileManager)
        var completed = false
        defer { if !completed { destination.removeIfOwned() } }
        let bundleURL = destination.root.appendingPathComponent("bundle", isDirectory: true)
        let layout = BundleLayout(bundleURL: bundleURL)
        try fileManager.createDirectory(
            at: layout.diskURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try fileManager.createDirectory(
            at: layout.varsURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try fileManager.createDirectory(at: layout.vtpmURL, withIntermediateDirectories: true)

        try HvfMediaImport.copy(disk: inputs.diskPath, vars: inputs.varsPath,
            toDisk: layout.diskURL, toVars: layout.varsURL, helper: snapshotHelper, fileManager: fileManager,
            converting: converting)
        let keyCustody = try FirstRunImportVTPMRecovery.materialize(inputs, destinationStableVMID: slug,
            destination: layout.vtpmURL, fileManager: fileManager)

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
        completed = true
        return FirstRunImportPreparation(config: config, destination: destination, importedVTPMKey: keyCustody)
    }
}
