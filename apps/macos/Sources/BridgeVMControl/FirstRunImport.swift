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
        case emptyName, invalidName, installerISO
        /// The Create sheet's sentence for the disk and UEFI variables files.
        case media(String)
        case unsupportedDiskFormat(String)
        case vtpmNotADirectory(String), vtpmRecoveryMissing(String), vtpmRecoveryWithoutState
        case vtpmRecoveryPackageRequired, vtpmRecoveryCodeRequired, vtpmRecoveryCodeShared(String)
        case badResources(memMiB: Int, cpuCount: Int)

        var errorDescription: String? { description }

        var description: String {
            switch self {
            case .emptyName: return "Enter a name for the VM."
            case .invalidName: return VMLibrary.vmNameRule
            case .installerISO: return "This is a Windows installer ISO. Use Install Windows instead."
            case .media(let sentence): return sentence
            case .unsupportedDiskFormat(let format):
                return "To import a \(format) disk, convert it to RAW, QCOW2, fixed-size VHD, or VHDX and choose the converted file."
            case .vtpmNotADirectory(let p): return "Choose the folder that holds the original TPM data. There’s no folder at \(p)."
            case .vtpmRecoveryMissing(let p): return "BridgeVM can’t use the recovery file at \(p). Choose the original file again."
            case .vtpmRecoveryPackageRequired: return "Choose the TPM recovery package."
            case .vtpmRecoveryCodeRequired: return "Choose the TPM recovery code file."
            case .vtpmRecoveryCodeShared(let p):
                return "Other accounts on this Mac can open the recovery code file at \(p). In Finder, choose File > Get Info and set everyone except you to No Access under Sharing & Permissions, then import again."
            case .vtpmRecoveryWithoutState: return "Choose the TPM data folder to use these recovery files."
            case .badResources(let mem, let cpu):
                return "Choose at least 2048 MiB of memory and 1 to 64 CPUs. The current settings are \(mem) MiB and \(cpu) CPUs."
            }
        }
    }

    static let requiredVarsBytes: UInt64 = 64 * 1024 * 1024

    /// Validate user inputs with the Create sheet's name and file checks, plus the
    /// first-run resource, installer ISO and vTPM checks.
    static func validate(
        _ inputs: Inputs,
        fileManager: FileManager = .default
    ) -> ValidationError? {
        if inputs.displayName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return .emptyName
        }
        if VMLibrary.normalizedVMName(inputs.displayName) == nil { return .invalidName }
        if inputs.memMiB < 2048 || !HvfEngineConfig.supportedCPURange.contains(inputs.cpuCount) {
            return .badResources(memMiB: inputs.memMiB, cpuCount: inputs.cpuCount)
        }
        if let sentence = VMLibrary.windowsHVFImportError(targetDiskPath: inputs.diskPath, varsPath: inputs.varsPath) {
            return .media(sentence)
        }
        if isInstallerISO(inputs.diskPath) { return .installerISO }
        if let error = FirstRunImportVTPMValidation.validate(inputs, fileManager: fileManager) { return error }
        return nil
    }

    /// An ISO 9660 volume descriptor ("CD001" at byte 0x8001), as on every Windows installer ISO.
    /// Called after the disk check, so the path is a readable regular file.
    private static func isInstallerISO(_ path: String) -> Bool {
        guard let handle = try? FileHandle(forReadingFrom: URL(fileURLWithPath: path)) else { return false }
        defer { try? handle.close() }
        guard (try? handle.seek(toOffset: 0x8001)) != nil else { return false }
        return (try? handle.read(upToCount: 5)) == Data("CD001".utf8)
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
        let keyCustody = try FirstRunImportVTPMRecovery.materialize(inputs, destinationStableVMID: destination.slug,
            destination: layout.vtpmURL, fileManager: fileManager)

        var config = VMConfig(
            id: destination.slug,
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
