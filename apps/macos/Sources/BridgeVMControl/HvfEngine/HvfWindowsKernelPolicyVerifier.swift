import CryptoKit
import Darwin
import Foundation

enum HvfWindowsKernelPolicyVerifier {
    static let attestationName = "bridgevm-kernel-policy-attestation.json"
    static let signatureName = "bridgevm-kernel-policy-attestation.sig"
    static let reportName = "bridgevm-finalization-report.txt"
    private static let policy = "windows-kernel-policy"
    private static let maximumAttestationBytes = 64 * 1024
    private static let maximumReportBytes = 64 * 1024
    private static let maximumArtifactBytes: UInt64 = 256 * 1024 * 1024
    private static let maximumPackageBytes: UInt64 = 512 * 1024 * 1024
    private static let maximumValidity: TimeInterval = 366 * 24 * 60 * 60

    struct Artifact: Codable, Equatable {
        let fileName: String
        let sha256: String
    }

    struct Attestation: Codable, Equatable {
        let artifacts: [Artifact]
        let expiresAt: String
        let issuedAt: String
        let keyID: String
        let packageID: String
        let policy: String
        let schemaVersion: Int
    }

    struct TrustAnchor: Equatable {
        let keyID: String
        let publicKeyBase64: String
        let notBefore: String
        let notAfter: String
        let revoked: Bool
    }

    struct VerifiedPackage: Equatable {
        let attestation: Attestation
        let attestationSHA256: String
    }

    struct ReportInspection: Equatable {
        let blocker: String?
        let signingMode: String
        let testSigningRequired: Bool?
    }

    enum Failure: String, Error, Equatable {
        case attestationMissing = "kernel-policy-provenance-unverifiable"
        case attestationInvalid = "provenance-attestation-invalid"
        case signatureInvalid = "provenance-signature-invalid"
        case trustAnchorUnknown = "provenance-trust-anchor-unknown"
        case trustAnchorRevoked = "provenance-trust-anchor-revoked"
        case timeInvalid = "provenance-time-invalid"
        case inventoryInvalid = "provenance-package-inventory-invalid"
        case hashMismatch = "provenance-package-hash-mismatch"
        case reportPolicyInvalid = "provenance-report-policy-invalid"
        case snapshotInvalid = "provenance-snapshot-invalid"
    }

    // Private material is never in the repository. Rotation adds a new key id
    // before this anchor expires; revocation flips `revoked` in a product update
    // and takes precedence over every signature and validity check.
    static let productionTrustAnchors = [
        TrustAnchor(
            keyID: "bridgevm-kernel-policy-2026-01",
            publicKeyBase64: "JoFvcf9P9qvU3tvW7DPCGFEUmo713CORBOagzSmG2GA=",
            notBefore: "2026-08-25T00:00:00Z",
            notAfter: "2028-08-25T00:00:00Z",
            revoked: false
        ),
    ]

    static func verify(
        packageDirectory: URL,
        now: Date = Date(),
        trustAnchors: [TrustAnchor] = productionTrustAnchors
    ) -> Result<VerifiedPackage, Failure> {
        do {
            return .success(try verifiedPackage(
                at: packageDirectory, now: now, trustAnchors: trustAnchors))
        } catch let failure as Failure {
            return .failure(failure)
        } catch {
            return .failure(.attestationInvalid)
        }
    }

    static func inspectReport(packageDirectory: URL) -> ReportInspection {
        let report = packageDirectory.appendingPathComponent(reportName)
        guard let data = try? boundedData(
            at: report, maximumBytes: maximumReportBytes,
            failure: .reportPolicyInvalid) else {
            return .init(blocker: "signing-report-missing", signingMode: "unknown",
                         testSigningRequired: nil)
        }
        guard let fields = try? reportFields(data) else {
            return .init(blocker: "signing-report-invalid", signingMode: "unknown",
                         testSigningRequired: nil)
        }
        guard fields["finalization_complete"] == "true",
              let mode = fields["signing_mode"],
              let requiredText = fields["test_signing_required"],
              let required = Bool(requiredText) else {
            return .init(blocker: "signing-report-invalid", signingMode: "unknown",
                         testSigningRequired: nil)
        }
        if required || mode == "test" {
            return .init(blocker: "test-signing-blocked-by-secure-boot", signingMode: mode,
                         testSigningRequired: required)
        }
        guard mode == "kernel-policy",
              fields["sys_kernel_policy_verified"] == "true",
              fields["cat_kernel_policy_verified"] == "true" else {
            return .init(blocker: "kernel-policy-unverifiable", signingMode: mode,
                         testSigningRequired: required)
        }
        return .init(blocker: nil, signingMode: mode, testSigningRequired: required)
    }

    static func stageVerifiedSnapshot(
        from source: URL,
        to destination: URL,
        now: Date = Date(),
        trustAnchors: [TrustAnchor] = productionTrustAnchors,
        copyFile: ((URL, URL) throws -> Void)? = nil
    ) -> Result<VerifiedPackage, Failure> {
        let initial: VerifiedPackage
        switch verify(packageDirectory: source, now: now, trustAnchors: trustAnchors) {
        case .success(let package): initial = package
        case .failure(let failure): return .failure(failure)
        }
        let fm = FileManager.default
        let parent = destination.deletingLastPathComponent()
        let temporary = parent.appendingPathComponent(
            ".\(destination.lastPathComponent).verifying.\(UUID().uuidString)", isDirectory: true)
        guard !fm.fileExists(atPath: destination.path),
              !fm.fileExists(atPath: temporary.path) else { return .failure(.snapshotInvalid) }
        do {
            try fm.createDirectory(
                at: temporary, withIntermediateDirectories: false,
                attributes: [.posixPermissions: 0o700])
            let names = initial.attestation.artifacts.map(\.fileName)
                + [attestationName, signatureName]
            for name in names {
                let sourceFile = source.appendingPathComponent(name)
                let destinationFile = temporary.appendingPathComponent(name)
                if let copyFile {
                    try copyFile(sourceFile, destinationFile)
                } else {
                    try fm.copyItem(at: sourceFile, to: destinationFile)
                }
            }
            let copied: VerifiedPackage
            switch verify(packageDirectory: temporary, now: now, trustAnchors: trustAnchors) {
            case .success(let package): copied = package
            case .failure: throw Failure.snapshotInvalid
            }
            guard copied == initial, !fm.fileExists(atPath: destination.path) else {
                throw Failure.snapshotInvalid
            }
            try fm.moveItem(at: temporary, to: destination)
            return .success(copied)
        } catch let failure as Failure {
            try? fm.removeItem(at: temporary)
            return .failure(failure)
        } catch {
            try? fm.removeItem(at: temporary)
            return .failure(.snapshotInvalid)
        }
    }

    private static func verifiedPackage(
        at directory: URL,
        now: Date,
        trustAnchors: [TrustAnchor]
    ) throws -> VerifiedPackage {
        let entries = try flatRegularEntries(at: directory)
        guard let attestationURL = entries[attestationName],
              let signatureURL = entries[signatureName] else { throw Failure.attestationMissing }
        let attestationData = try boundedData(
            at: attestationURL, maximumBytes: maximumAttestationBytes,
            failure: .attestationInvalid)
        let signature = try boundedData(at: signatureURL, maximumBytes: 64, failure: .signatureInvalid)
        guard signature.count == 64 else { throw Failure.signatureInvalid }

        let decoder = JSONDecoder()
        guard let attestation = try? decoder.decode(Attestation.self, from: attestationData),
              attestationData == canonicalData(attestation),
              attestation.schemaVersion == 1,
              attestation.policy == policy,
              validToken(attestation.packageID),
              validToken(attestation.keyID) else { throw Failure.attestationInvalid }

        let matchingAnchors = trustAnchors.filter { $0.keyID == attestation.keyID }
        guard matchingAnchors.count == 1, let anchor = matchingAnchors.first else {
            throw Failure.trustAnchorUnknown
        }
        guard !anchor.revoked else { throw Failure.trustAnchorRevoked }
        let issuedAt = try strictDate(attestation.issuedAt)
        let expiresAt = try strictDate(attestation.expiresAt)
        let anchorStart = try strictDate(anchor.notBefore)
        let anchorEnd = try strictDate(anchor.notAfter)
        guard issuedAt <= now, now <= expiresAt, issuedAt < expiresAt,
              expiresAt.timeIntervalSince(issuedAt) <= maximumValidity,
              anchorStart <= issuedAt, expiresAt <= anchorEnd else { throw Failure.timeInvalid }

        guard let rawKey = Data(base64Encoded: anchor.publicKeyBase64), rawKey.count == 32,
              let publicKey = try? Curve25519.Signing.PublicKey(rawRepresentation: rawKey),
              publicKey.isValidSignature(signature, for: attestationData) else {
            throw Failure.signatureInvalid
        }

        try validateInventory(attestation.artifacts, entries: entries)
        try validateReport(in: directory, artifacts: attestation.artifacts)
        return VerifiedPackage(
            attestation: attestation,
            attestationSHA256: sha256(attestationData)
        )
    }

    private static func flatRegularEntries(at directory: URL) throws -> [String: URL] {
        let rootValues = try directory.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        guard rootValues.isDirectory == true, rootValues.isSymbolicLink != true else {
            throw Failure.inventoryInvalid
        }
        let keys: Set<URLResourceKey> = [
            .isRegularFileKey, .isDirectoryKey, .isSymbolicLinkKey, .fileSizeKey,
        ]
        let urls = try FileManager.default.contentsOfDirectory(
            at: directory, includingPropertiesForKeys: Array(keys), options: [])
        guard urls.count <= 66 else { throw Failure.inventoryInvalid }
        var result: [String: URL] = [:]
        var folded = Set<String>()
        for url in urls {
            let values = try url.resourceValues(forKeys: keys)
            let name = url.lastPathComponent
            guard values.isRegularFile == true, values.isDirectory != true,
                  values.isSymbolicLink != true, validFileName(name),
                  folded.insert(name.lowercased()).inserted,
                  result.updateValue(url, forKey: name) == nil else {
                throw Failure.inventoryInvalid
            }
        }
        return result
    }

    private static func validateInventory(
        _ artifacts: [Artifact],
        entries: [String: URL]
    ) throws {
        guard !artifacts.isEmpty, artifacts.count <= 64,
              artifacts == artifacts.sorted(by: { $0.fileName < $1.fileName }),
              artifacts.contains(where: { $0.fileName == reportName }),
              artifacts.contains(where: { $0.fileName.lowercased().hasSuffix(".inf") }),
              artifacts.contains(where: { $0.fileName.lowercased().hasSuffix(".sys") }),
              artifacts.contains(where: { $0.fileName.lowercased().hasSuffix(".cat") }) else {
            throw Failure.inventoryInvalid
        }
        let expectedNames = Set(artifacts.map(\.fileName) + [attestationName, signatureName])
        guard expectedNames.count == artifacts.count + 2,
              expectedNames == Set(entries.keys) else { throw Failure.inventoryInvalid }
        var folded = Set<String>()
        var totalBytes: UInt64 = 0
        for artifact in artifacts {
            guard validFileName(artifact.fileName), validHash(artifact.sha256),
                  folded.insert(artifact.fileName.lowercased()).inserted,
                  let url = entries[artifact.fileName] else { throw Failure.inventoryInvalid }
            let measured = try hashFile(url, maximumBytes: maximumArtifactBytes)
            guard totalBytes <= maximumPackageBytes - measured.size else {
                throw Failure.inventoryInvalid
            }
            totalBytes += measured.size
            guard measured.hash == artifact.sha256 else { throw Failure.hashMismatch }
        }
    }

    private static func validateReport(in directory: URL, artifacts: [Artifact]) throws {
        let reportURL = directory.appendingPathComponent(reportName)
        let data = try boundedData(
            at: reportURL, maximumBytes: maximumReportBytes,
            failure: .reportPolicyInvalid)
        let fields = try reportFields(data)
        guard fields["finalization_complete"] == "true",
              fields["signing_mode"] == "kernel-policy",
              fields["test_signing_required"] == "false",
              fields["sys_kernel_policy_verified"] == "true",
              fields["cat_kernel_policy_verified"] == "true" else {
            throw Failure.reportPolicyInvalid
        }
        for artifact in artifacts where artifact.fileName != reportName {
            guard fields["sha256.\(artifact.fileName)"] == artifact.sha256 else {
                throw Failure.reportPolicyInvalid
            }
        }
    }

    private static func reportFields(_ data: Data) throws -> [String: String] {
        guard let text = String(data: data, encoding: .ascii) else {
            throw Failure.reportPolicyInvalid
        }
        var fields: [String: String] = [:]
        for rawLine in text.split(whereSeparator: \.isNewline) {
            let line = String(rawLine)
            guard let separator = line.firstIndex(of: "=") else { continue }
            let key = String(line[..<separator])
            let value = String(line[line.index(after: separator)...])
            guard !key.isEmpty, fields.updateValue(value, forKey: key) == nil else {
                throw Failure.reportPolicyInvalid
            }
        }
        return fields
    }

    static func canonicalData(_ attestation: Attestation) -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        var data = (try? encoder.encode(attestation)) ?? Data()
        data.append(0x0a)
        return data
    }

    private static func boundedData(
        at url: URL,
        maximumBytes: Int,
        failure: Failure
    ) throws -> Data {
        let descriptor = Darwin.open(url.path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW)
        guard descriptor >= 0 else { throw failure }
        defer { Darwin.close(descriptor) }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              status.st_mode & S_IFMT == S_IFREG,
              status.st_size >= 0, status.st_size <= maximumBytes else { throw failure }
        var data = Data(count: Int(status.st_size))
        var offset = 0
        while offset < data.count {
            let remaining = data.count - offset
            let count = data.withUnsafeMutableBytes { buffer in
                Darwin.read(descriptor, buffer.baseAddress?.advanced(by: offset), remaining)
            }
            if count < 0, errno == EINTR { continue }
            guard count > 0 else { throw failure }
            offset += count
        }
        var extra: UInt8 = 0
        guard Darwin.read(descriptor, &extra, 1) == 0 else { throw failure }
        return data
    }

    private static func hashFile(
        _ url: URL,
        maximumBytes: UInt64
    ) throws -> (hash: String, size: UInt64) {
        let descriptor = Darwin.open(url.path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW)
        guard descriptor >= 0 else { throw Failure.inventoryInvalid }
        defer { Darwin.close(descriptor) }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              status.st_mode & S_IFMT == S_IFREG, status.st_size >= 0,
              UInt64(status.st_size) <= maximumBytes else {
            throw Failure.inventoryInvalid
        }
        var hasher = SHA256()
        var buffer = [UInt8](repeating: 0, count: 1024 * 1024)
        var bytesRead: UInt64 = 0
        while true {
            let count = buffer.withUnsafeMutableBytes {
                Darwin.read(descriptor, $0.baseAddress, $0.count)
            }
            if count < 0, errno == EINTR { continue }
            guard count >= 0 else { throw Failure.inventoryInvalid }
            if count == 0 { break }
            guard bytesRead <= maximumBytes - UInt64(count) else {
                throw Failure.inventoryInvalid
            }
            bytesRead += UInt64(count)
            hasher.update(data: Data(buffer[0..<count]))
        }
        let hash = hasher.finalize().map { String(format: "%02x", $0) }.joined()
        return (hash, bytesRead)
    }

    private static func sha256(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

    private static func strictDate(_ text: String) throws -> Date {
        guard text.range(
            of: #"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$"#,
            options: .regularExpression) != nil else { throw Failure.timeInvalid }
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime]
        guard let date = formatter.date(from: text) else { throw Failure.timeInvalid }
        return date
    }

    private static func validToken(_ value: String) -> Bool {
        value.count <= 128 && value.range(
            of: #"^[A-Za-z0-9][A-Za-z0-9._-]*$"#, options: .regularExpression) != nil
    }

    private static func validFileName(_ value: String) -> Bool {
        value.count <= 128 && value.range(
            of: #"^[A-Za-z0-9][A-Za-z0-9._-]*$"#, options: .regularExpression) != nil
    }

    private static func validHash(_ value: String) -> Bool {
        value.range(of: #"^[0-9a-f]{64}$"#, options: .regularExpression) != nil
    }
}
