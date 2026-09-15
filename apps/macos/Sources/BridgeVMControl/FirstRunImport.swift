import Foundation

/// D2 first-run import: register an existing HVF Windows VM bundle (an installed
/// raw disk + 64 MiB UEFI vars + optional vTPM state dir) into the library so a
/// cold GUI start can boot it. Import-only: no ISO install, no from-scratch VM
/// creation — those stay separate flows. Validation is fail-closed at the
/// system boundary (user-picked paths), mirroring VTPMStateSecurity's stance.
enum FirstRunImport {
    /// The three inputs a user selects. Only the disk and vars are required; a
    /// vTPM state dir is optional (a fresh TPM is initialized when absent).
    struct Inputs: Equatable, Sendable {
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
    /// selected inputs into it. Disk and vars are independent copies so guest
    /// writes cannot modify the source or another imported VM.
    /// Returns the configuration for the caller to persist.
    static func register(
        _ inputs: Inputs,
        slug: String,
        libraryRoot: URL,
        fileManager: FileManager = .default, snapshotHelper: URL = HvfMediaImportHelper.bundled
    ) throws -> VMConfig {
        let prepared = try prepare(inputs, slug: slug, libraryRoot: libraryRoot,
            fileManager: fileManager, snapshotHelper: snapshotHelper)
        prepared.preserve()
        return prepared.config
    }
}
