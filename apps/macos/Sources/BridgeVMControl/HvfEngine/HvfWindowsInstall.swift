import Foundation

// MARK: - Install request (persisted per VM bundle)

/// A pending Windows HVF install described at VM-creation time and executed
/// later from the VM detail panel. Persisted as metadata/hvf-install.json so
/// an interrupted install can be retried after an app relaunch.
struct HvfWindowsInstallRequest: Codable, Equatable, Sendable {
    var isoPath: String
    var isoSHA256: String? = nil
    var diskGiB: Int
    var injectViogpu3d: Bool
    var driverPackageDir: String?
    var guestPayloadDirectory: String? = nil
    var guestPayloadManifest: String? = nil
    var guestPayloadIdentity: String? = nil
    var unattendedPath: String? = nil
    var unattendedIdentity: String? = nil

    static let fileName = "metadata/hvf-install.json"
    static let doneFileName = "metadata/hvf-install-done.json"

    static func load(bundlePath: String) -> HvfWindowsInstallRequest? {
        let url = URL(fileURLWithPath: bundlePath).appendingPathComponent(fileName)
        guard let data = FileManager.default.contents(atPath: url.path) else { return nil }
        return try? JSONDecoder().decode(HvfWindowsInstallRequest.self, from: data)
    }
    @discardableResult
    func save(bundlePath: String) -> Bool {
        let url = URL(fileURLWithPath: bundlePath).appendingPathComponent(Self.fileName)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        guard let data = try? encoder.encode(self) else { return false }
        do {
            try FileManager.default.createDirectory(
                at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try data.write(to: url, options: [.atomic])
            return true
        } catch {
            return false
        }
    }
}

// MARK: - Install plan (pure path/argument computation, unit-testable)

struct HvfWindowsInstallPlan: Equatable, Sendable {
    let repoRoot: URL
    let libraryRoot: URL
    let bundlePath: String
    let slug: String
    let request: HvfWindowsInstallRequest
    let sealedISOSHA256: String
    let sealedGuestPayloadIdentity: String
    let sealedUnattendedIdentity: String
    let sourceCacheKey: String

    init(repoRoot: URL, libraryRoot: URL = VMLibrary.root, bundlePath: String,
         slug: String, request: HvfWindowsInstallRequest) {
        self.repoRoot = repoRoot
        self.libraryRoot = libraryRoot
        self.bundlePath = bundlePath
        self.slug = slug
        self.request = request
        sealedISOSHA256 = request.isoSHA256
            ?? HvfWindowsInstallCacheIdentity.sha256File(request.isoPath) ?? "absent"
        sealedGuestPayloadIdentity = request.guestPayloadIdentity ?? "absent"
        sealedUnattendedIdentity = request.unattendedIdentity ?? "default"
        sourceCacheKey = HvfWindowsInstallCacheIdentity.key(
            isoSHA256: sealedISOSHA256,
            guestPayloadIdentity: sealedGuestPayloadIdentity,
            unattendedIdentity: sealedUnattendedIdentity,
            repoRoot: repoRoot)
    }
    static let minimumDiskGiB = 64
    static let installResourcePaths = [
        "scripts/build-hvf-windows-scripted-source.sh",
        "scripts/stage-hvf-windows-guest-payload.sh",
        "scripts/hvf-disk-image-utils.sh",
        "scripts/run-hvf-windows-scripted-install.sh",
        "scripts/verify-hvf-windows-install-target.sh",
        "target/release/examples/hvf_gic_boot_probe",
        "scripts/win-assets/winpeshl.ini",
        "scripts/win-assets/bvinstall.cmd",
        "scripts/win-assets/bvdiskpart.txt",
        "helpers/bv-file-compare.exe",
        "scripts/win-assets/unattend.xml",
        "helpers/bridgevm-catalog-verify",
    ] + HvfWindowsAgentAssets.requiredPaths

    var sourceImagePath: String {
        libraryRoot.appendingPathComponent(
            "Derived/WindowsInstallSources/\(sourceCacheKey).raw").path
    }
    var wimlibPath: String? { HvfWindowsWimlib.resolve(repoRoot: repoRoot) }
    var catalogVerifierPath: String? {
        HvfWindowsCatalogVerifier.resolve(repoRoot: repoRoot)
    }
    var fileComparePath: String { repoRoot.appendingPathComponent("helpers/bv-file-compare.exe").path }
    var tmpTargetPath: String { "/tmp/bridgevm-appinstall-\(slug)-target.raw" }
    var tmpVarsPath: String { "/tmp/bridgevm-appinstall-\(slug)-vars.fd" }
    var tmpEvidenceDir: String { "/tmp/bridgevm-appinstall-\(slug)-evidence" }

    var bundleDiskPath: String { "\(bundlePath)/disks/hvf-target.raw" }
    var bundleVarsPath: String { "\(bundlePath)/metadata/hvf-vars.fd" }
    var bundleInstallLogPath: String { "\(bundlePath)/logs/install-run.log" }

    var freshTargetSizeBytes: UInt64 { UInt64(request.diskGiB) * 1024 * 1024 * 1024 }

    // MARK: commands

    /// Stage a: host-side WinPE scripted-installer source build from the ISO.
    func sourceBuildCommand() -> (environment: [String: String], arguments: [String]) {
        (
            environment: [
                "ISO": request.isoPath,
                "OUT": sourceImagePath,
                "WIMLIB": wimlibPath ?? "",
                "WINDOWS_GUEST_PAYLOAD_DIR": request.guestPayloadDirectory ?? "",
                "WINDOWS_GUEST_PAYLOAD_MANIFEST": request.guestPayloadManifest ?? "",
                "WINDOWS_GUEST_PAYLOAD_CATALOG_VERIFIER": catalogVerifierPath ?? "",
                "WINDOWS_FILE_COMPARE": fileComparePath,
                "WINDOWS_UNATTEND_PATH": request.unattendedPath ?? "",
            ],
            arguments: ["/bin/bash", "scripts/build-hvf-windows-scripted-source.sh"]
        )
    }

    /// Stage c: the unattended scripted install boot (WIM apply + bcdboot +
    /// unattended OOBE; reboots into the installed OS before exiting).
    func installCommand() -> [String] {
        let arguments = [
            "/bin/bash", "scripts/run-hvf-windows-scripted-install.sh",
            "--source", sourceImagePath,
            "--target", tmpTargetPath,
            "--fresh-target-size", String(freshTargetSizeBytes),
            "--vars", tmpVarsPath,
            "--evidence-dir", tmpEvidenceDir,
            "--release",
            "--skip-build",
            "--watchdog-ms", "1500000",
        ]
        return arguments
    }

    // MARK: validation

    static func whitespaceFree(_ path: String) -> Bool {
        path.rangeOfCharacter(from: .whitespacesAndNewlines) == nil
    }

    static func driverPackageError(_ directory: String) -> String? {
        let fm = FileManager.default
        var isDirectory: ObjCBool = false
        guard fm.fileExists(atPath: directory, isDirectory: &isDirectory), isDirectory.boolValue else {
            return "viogpu3d 드라이버 패키지 폴더를 찾을 수 없습니다."
        }
        guard whitespaceFree(directory) else {
            return "드라이버 패키지 경로에 공백이 있으면 인젝터를 만들 수 없습니다."
        }
        let entries = (try? fm.contentsOfDirectory(atPath: directory)) ?? []
        let lowered = entries.map { $0.lowercased() }
        guard lowered.contains(where: { $0.hasSuffix(".inf") }) else {
            return "드라이버 패키지 폴더에 .inf 파일이 없습니다."
        }
        guard lowered.contains(where: { $0.hasSuffix(".sys") }) else {
            return "드라이버 패키지 폴더에 .sys 파일이 없습니다."
        }
        return HvfWindowsDriverPreflight.inspect(packageDirectory: directory).userMessage
    }

    func validationError() -> String? {
        if let blocker = VMLibrary.windowsHVFInjectionError(requested: request.injectViogpu3d) {
            return blocker
        }
        let fm = FileManager.default
        guard fm.isReadableFile(atPath: request.isoPath) else {
            return "Windows 11 ARM64 ISO 파일을 찾을 수 없습니다."
        }
        guard HvfWindowsInstallCacheIdentity.sha256File(request.isoPath) == sealedISOSHA256 else {
            return "선택한 Windows ISO가 VM 생성 후 변경되었습니다."
        }
        guard HvfWindowsInstallCacheIdentity.key(
                isoSHA256: sealedISOSHA256,
                guestPayloadIdentity: sealedGuestPayloadIdentity,
                unattendedIdentity: sealedUnattendedIdentity,
                repoRoot: repoRoot)
                == sourceCacheKey else { return "Windows 설치 레시피 또는 wimlib가 변경되었습니다." }
        let requiredResources = Self.installResourcePaths
        if let missing = requiredResources.first(where: {
            !fm.isReadableFile(atPath: repoRoot.appendingPathComponent($0).path)
        }) {
            return "앱 설치 리소스가 없습니다: \(missing)"
        }
        guard request.diskGiB >= Self.minimumDiskGiB else {
            return "디스크 크기는 최소 \(Self.minimumDiskGiB) GiB여야 합니다."
        }
        let expectedPayloadDirectory = "\(bundlePath)/metadata/windows-guest-payload"
        let expectedPayloadManifest = "\(bundlePath)/metadata/windows-guest-payload.tsv"
        guard request.guestPayloadDirectory == expectedPayloadDirectory,
              request.guestPayloadManifest == expectedPayloadManifest else {
            return "게스트 드라이버 payload는 VM 번들에 봉인된 경로만 사용할 수 있습니다."
        }
        let payload = HvfWindowsGuestPayloadIdentity.inspect(
            payloadDirectory: request.guestPayloadDirectory,
            manifestPath: request.guestPayloadManifest)
        if let error = payload.error { return error }
        guard payload.digest == sealedGuestPayloadIdentity else {
            return "VM 생성 후 게스트 드라이버 payload 또는 manifest가 변경되었습니다."
        }
        if request.unattendedPath != nil || request.unattendedIdentity != nil {
            let expected = "\(bundlePath)/metadata/windows-install-unattend.xml"
            guard request.unattendedPath == expected,
                  request.unattendedIdentity == sealedUnattendedIdentity,
                  HvfWindowsInstallCacheIdentity.sha256File(expected)
                    == sealedUnattendedIdentity else {
                return "E2E 설치 응답 파일이 VM 번들에 봉인되지 않았거나 변경되었습니다."
            }
        }
        guard wimlibPath != nil else {
            return "앱에 서명·봉인된 wimlib-imagex helper가 없습니다. 앱을 다시 설치하세요."
        }
        guard catalogVerifierPath != nil else {
            return "앱에 서명·봉인된 Windows catalog verifier가 없습니다. 앱을 다시 설치하세요."
        }
        return nil
    }
}

/// Thread-safe newline splitter for Process pipe callbacks.
final class LineAccumulator: @unchecked Sendable {
    private var buffer = Data()
    private let lock = NSLock()

    /// Split on newlines in one pass, dropping the consumed prefix once.
    ///
    /// Rebuilding the buffer per line copied every remaining byte each time,
    /// so a single large pipe delivery cost time quadratic in its size: 40,000
    /// lines took 497 ms that way against 7 ms here. Installer output arrives
    /// in exactly those bursts.
    func append(_ data: Data) -> [String] {
        lock.lock()
        defer { lock.unlock() }
        buffer.append(data)
        var lines: [String] = []
        var start = buffer.startIndex
        while let newline = buffer[start...].firstIndex(of: 0x0a) {
            let lineData = buffer[start..<newline]
            if let line = String(data: lineData, encoding: .utf8), !line.isEmpty {
                lines.append(line)
            }
            start = buffer.index(after: newline)
        }
        buffer.removeSubrange(..<start)
        return lines
    }
}
