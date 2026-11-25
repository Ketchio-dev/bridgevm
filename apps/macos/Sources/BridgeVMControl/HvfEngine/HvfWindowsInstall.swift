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
        guard let data = try? HvfWindowsInstallDurability.readRegularFile(url, maximumBytes: VMLibrary.maximumConfigBytes) else { return nil }
        return try? JSONDecoder().decode(HvfWindowsInstallRequest.self, from: data)
    }
    @discardableResult
    func save(bundlePath: String) -> Bool {
        let url = URL(fileURLWithPath: bundlePath).appendingPathComponent(Self.fileName)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        guard let data = try? encoder.encode(self), data.count <= VMLibrary.maximumConfigBytes else { return false }
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
    var sourceImagePath: String {
        libraryRoot.appendingPathComponent(
            "Derived/WindowsInstallSources/\(sourceCacheKey).raw").path
    }
    var wimlibPath: String? { HvfWindowsWimlib.resolve(repoRoot: repoRoot) }
    var catalogVerifierPath: String? {
        HvfWindowsCatalogVerifier.resolve(repoRoot: repoRoot)
    }
    var fileComparePath: String { repoRoot.appendingPathComponent("helpers/bv-file-compare.exe").path }

    var bundleDiskPath: String { "\(bundlePath)/disks/hvf-target.raw" }
    var bundleVarsPath: String { "\(bundlePath)/metadata/hvf-vars.fd" }

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
            "--target", stagingTargetPath,
            "--vars", stagingVarsPath,
            "--evidence-dir", stagingEvidenceDir,
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

    private var isoRecoveryHint: String {
        let relativePath = WindowsHVFProductPolicy.installerISO
        let location = request.isoPath == bundlePath + "/" + relativePath ? "VM 번들의 \(relativePath)" : "원래 위치"
        return "VM 생성에 사용한 원본 ISO를 \(location)에 복원한 뒤 다시 시도하세요."
    }

    func validationError() -> String? {
        if let blocker = VMLibrary.windowsHVFInjectionError(requested: request.injectViogpu3d) {
            return blocker
        }
        let fm = FileManager.default
        guard fm.isReadableFile(atPath: request.isoPath) else {
            return "Windows 설치 ISO를 읽을 수 없습니다. \(isoRecoveryHint)"
        }
        guard HvfWindowsInstallCacheIdentity.sha256File(request.isoPath) == sealedISOSHA256 else {
            return "Windows 설치 ISO가 VM 생성 때와 다릅니다. \(isoRecoveryHint)"
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
        if let error = Self.diskSizeError(request.diskGiB) { return error }
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
                return "설치 응답 파일이 VM 번들에 봉인되지 않았거나 변경되었습니다."
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

extension HvfWindowsInstallPlan {
    static let unrepresentableDiskSizeMessage = "설치 디스크 크기를 파일 크기로 표현할 수 없습니다."

    /// FileHandle truncation ultimately uses signed off_t, not the full UInt64 range.
    /// This is a representation boundary, not a capacity or workload-fit guarantee.
    static func targetSizeBytes(diskGiB: Int) -> UInt64? {
        let bytesPerGiB: UInt64 = 1 << 30
        guard diskGiB > 0, let size = UInt64(exactly: diskGiB),
              size <= UInt64(Int64.max) / bytesPerGiB else { return nil }
        return size * bytesPerGiB
    }

    var freshTargetSizeBytes: UInt64? { Self.targetSizeBytes(diskGiB: request.diskGiB) }

    static func diskSizeError(_ diskGiB: Int) -> String? {
        guard diskGiB >= minimumDiskGiB else {
            return "디스크 크기는 최소 \(minimumDiskGiB) GiB여야 합니다."
        }
        return targetSizeBytes(diskGiB: diskGiB) == nil ? unrepresentableDiskSizeMessage : nil
    }
}

extension HvfWindowsInstallPlan {
    var sourceImageSHA256Path: String { sourceImagePath + ".sha256" }

    var sourceImageCacheCandidateExists: Bool {
        let manager = FileManager.default
        return manager.isReadableFile(atPath: sourceImagePath)
            && manager.isReadableFile(atPath: sourceImageSHA256Path)
    }

    func sourceImageCacheIsVerified() async -> Bool {
        guard sourceImageCacheCandidateExists else { return false }
        let imagePath = sourceImagePath
        let receiptPath = sourceImageSHA256Path
        return await Task.detached {
            guard let receipt = try? String(contentsOfFile: receiptPath, encoding: .utf8) else {
                return false
            }
            let expected = receipt.trimmingCharacters(in: .whitespacesAndNewlines)
            guard expected.count == 64,
                  expected.allSatisfy({ $0.isHexDigit && !$0.isUppercase }) else { return false }
            return HvfWindowsInstallCacheIdentity.sha256File(imagePath) == expected
        }.value
    }
}
