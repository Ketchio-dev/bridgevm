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
            return "Choose the folder containing the viogpu3d driver package."
        }
        guard whitespaceFree(directory) else {
            return "Choose a driver package folder with no spaces in its path."
        }
        let entries = (try? fm.contentsOfDirectory(atPath: directory)) ?? []
        let lowered = entries.map { $0.lowercased() }
        guard lowered.contains(where: { $0.hasSuffix(".inf") }) else {
            return "The driver package folder is missing an .inf file."
        }
        guard lowered.contains(where: { $0.hasSuffix(".sys") }) else {
            return "The driver package folder is missing a .sys file."
        }
        return HvfWindowsDriverPreflight.inspect(packageDirectory: directory).userMessage
    }

    private var isoRecoveryHint: String {
        let relativePath = WindowsHVFProductPolicy.installerISO
        let location = request.isoPath == bundlePath + "/" + relativePath ? "\(relativePath) in the VM bundle" : "its original location"
        return "Restore the ISO used to create this VM to \(location), then retry."
    }

    func validationError() -> String? {
        if let blocker = VMLibrary.windowsHVFInjectionError(requested: request.injectViogpu3d) {
            return blocker
        }
        let fm = FileManager.default
        guard fm.isReadableFile(atPath: request.isoPath) else {
            return "BridgeVM can’t read the Windows installation ISO. \(isoRecoveryHint)"
        }
        guard HvfWindowsInstallCacheIdentity.sha256File(request.isoPath) == sealedISOSHA256 else {
            return "The Windows installation ISO has changed since this VM was created. \(isoRecoveryHint)"
        }
        guard HvfWindowsInstallCacheIdentity.key(
                isoSHA256: sealedISOSHA256,
                guestPayloadIdentity: sealedGuestPayloadIdentity,
                unattendedIdentity: sealedUnattendedIdentity,
                repoRoot: repoRoot)
                == sourceCacheKey else { return "The Windows installation tools changed. Reopen BridgeVM and retry." }
        let requiredResources = Self.installResourcePaths
        if let missing = requiredResources.first(where: {
            !fm.isReadableFile(atPath: repoRoot.appendingPathComponent($0).path)
        }) {
            return "BridgeVM is missing an installation resource (\(missing)). Reinstall the app and retry."
        }
        if let error = Self.diskSizeError(request.diskGiB) { return error }
        let expectedPayloadDirectory = "\(bundlePath)/metadata/windows-guest-payload"
        let expectedPayloadManifest = "\(bundlePath)/metadata/windows-guest-payload.tsv"
        guard request.guestPayloadDirectory == expectedPayloadDirectory,
              request.guestPayloadManifest == expectedPayloadManifest else {
            return "Windows Setup needs the driver files stored with this VM."
        }
        let payload = HvfWindowsGuestPayloadIdentity.inspect(
            payloadDirectory: request.guestPayloadDirectory,
            manifestPath: request.guestPayloadManifest)
        if let error = payload.error { return error }
        guard payload.digest == sealedGuestPayloadIdentity else {
            return "The Windows driver files or their manifest have changed since this VM was created. Restore the original files and retry."
        }
        if request.unattendedPath != nil || request.unattendedIdentity != nil {
            let expected = "\(bundlePath)/metadata/windows-install-unattend.xml"
            guard request.unattendedPath == expected,
                  request.unattendedIdentity == sealedUnattendedIdentity,
                  HvfWindowsInstallCacheIdentity.sha256File(expected)
                    == sealedUnattendedIdentity else {
                return "The unattended setup file is missing from its saved location or has changed. Restore the original file and retry."
            }
        }
        guard wimlibPath != nil else {
            return "Reinstall BridgeVM to restore the verified wimlib-imagex installation tool."
        }
        guard catalogVerifierPath != nil else {
            return "Reinstall BridgeVM to restore the verified Windows driver catalog tool."
        }
        return nil
    }
}

extension HvfWindowsInstallPlan {
    static let unrepresentableDiskSizeMessage = "The requested Windows disk size exceeds the supported file size."

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
            return "Choose a Windows disk size of at least \(minimumDiskGiB) GiB."
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
