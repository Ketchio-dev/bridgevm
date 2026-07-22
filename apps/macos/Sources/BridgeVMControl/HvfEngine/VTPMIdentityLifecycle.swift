import CryptoKit
import Foundation
import Security

enum VTPMIdentityLifecycleError: LocalizedError {
    case invalidRecoveryCode
    case invalidRecoveryPackage
    case vmIdentityMismatch(expected: String, package: String)
    case stateFingerprintMismatch
    case unsafeStateEntry(String)
    case resetRollbackFailed(String)

    var errorDescription: String? {
        switch self {
        case .invalidRecoveryCode:
            return "vTPM 복구 코드가 올바른 256비트 코드가 아닙니다."
        case .invalidRecoveryPackage:
            return "vTPM 복구 패키지가 손상되었거나 코드와 일치하지 않습니다."
        case let .vmIdentityMismatch(expected, package):
            return "복구 패키지의 VM ID(\(package))가 현재 VM ID(\(expected))와 다릅니다."
        case .stateFingerprintMismatch:
            return "복구 패키지가 현재 vTPM 암호화 상태와 짝을 이루지 않습니다. 상태와 키를 함께 복원하세요."
        case let .unsafeStateEntry(path):
            return "vTPM 상태에 안전하지 않은 링크 또는 파일이 있습니다: \(path)"
        case let .resetRollbackFailed(message):
            return "vTPM 재설정 롤백에 실패했습니다: \(message)"
        }
    }
}

struct VTPMRecoveryExport {
    let recoveryCode: String
    let stateFingerprint: String
}

struct VTPMResetResult {
    let archivedStatePath: String?
    let receiptPath: String
}

private struct VTPMRecoveryPackage: Codable {
    let format: String
    let stableVMID: String
    let createdAt: String
    let stateFingerprint: String
    let nonce: String
    let ciphertext: String
    let tag: String
}

private struct VTPMLifecycleReceipt: Codable {
    let format: String
    let operation: String
    let stableVMID: String
    let occurredAt: String
    let previousStateFingerprint: String?
    let archivedStatePath: String?
    let archivedKeyID: String?
    let recoveryPolicy: String
    let sourceBundlePath: String?
    let destinationBundlePath: String?
}

/// Explicit vTPM identity transitions. State never silently receives a new key:
/// migration uses an authenticated recovery package, while reset archives both
/// the old encrypted state and a device-local copy of its key before rotation.
struct VTPMIdentityLifecycle {
    static let recoveryFormat = "bridgevm-vtpm-recovery-v1"
    static let receiptFormat = "bridgevm-vtpm-lifecycle-v1"

    let keyStore: VTPMStateKeyManaging
    var fileManager: FileManager = .default

    func exportRecovery(
        stableVMID: String,
        stateDirectory: URL,
        destination: URL,
        now: Date = Date()
    ) throws -> VTPMRecoveryExport {
        let stateKey = try keyStore.stateKey(for: stableVMID, allowCreation: false)
        let fingerprint = try stateFingerprint(at: stateDirectory)
        let keyLength = KeychainVTPMStateKeyStore.keyLength
        var recoverySecret = Data(count: keyLength)
        let status = recoverySecret.withUnsafeMutableBytes { bytes in
            SecRandomCopyBytes(kSecRandomDefault, keyLength, bytes.baseAddress!)
        }
        guard status == errSecSuccess else {
            throw VTPMStateSecurityError.randomGeneration(status)
        }
        defer { recoverySecret.resetBytes(in: recoverySecret.indices) }

        let createdAt = Self.timestamp(now)
        let aad = Self.associatedData(
            stableVMID: stableVMID,
            createdAt: createdAt,
            stateFingerprint: fingerprint
        )
        let sealed = try AES.GCM.seal(
            stateKey,
            using: SymmetricKey(data: recoverySecret),
            authenticating: aad
        )
        let package = VTPMRecoveryPackage(
            format: Self.recoveryFormat,
            stableVMID: stableVMID,
            createdAt: createdAt,
            stateFingerprint: fingerprint,
            nonce: Data(sealed.nonce).base64EncodedString(),
            ciphertext: sealed.ciphertext.base64EncodedString(),
            tag: sealed.tag.base64EncodedString()
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        try encoder.encode(package).write(to: destination, options: [.atomic])
        try fileManager.setAttributes([.posixPermissions: 0o600], ofItemAtPath: destination.path)
        return VTPMRecoveryExport(
            recoveryCode: Self.recoveryCode(for: recoverySecret),
            stateFingerprint: fingerprint
        )
    }

    func restoreRecovery(
        stableVMID: String,
        stateDirectory: URL,
        packageURL: URL,
        recoveryCode: String
    ) throws {
        var secret = try Self.decodeRecoveryCode(recoveryCode)
        defer { secret.resetBytes(in: secret.indices) }
        let packageData = try Data(contentsOf: packageURL, options: [.mappedIfSafe])
        let package: VTPMRecoveryPackage
        do {
            package = try JSONDecoder().decode(VTPMRecoveryPackage.self, from: packageData)
        } catch {
            throw VTPMIdentityLifecycleError.invalidRecoveryPackage
        }
        guard package.format == Self.recoveryFormat else {
            throw VTPMIdentityLifecycleError.invalidRecoveryPackage
        }
        guard package.stableVMID == stableVMID else {
            throw VTPMIdentityLifecycleError.vmIdentityMismatch(
                expected: stableVMID,
                package: package.stableVMID
            )
        }
        guard try stateFingerprint(at: stateDirectory) == package.stateFingerprint else {
            throw VTPMIdentityLifecycleError.stateFingerprintMismatch
        }
        guard let nonceData = Data(base64Encoded: package.nonce),
              let ciphertext = Data(base64Encoded: package.ciphertext),
              let tag = Data(base64Encoded: package.tag),
              let nonce = try? AES.GCM.Nonce(data: nonceData),
              let box = try? AES.GCM.SealedBox(nonce: nonce, ciphertext: ciphertext, tag: tag)
        else { throw VTPMIdentityLifecycleError.invalidRecoveryPackage }
        let aad = Self.associatedData(
            stableVMID: package.stableVMID,
            createdAt: package.createdAt,
            stateFingerprint: package.stateFingerprint
        )
        let stateKey: Data
        do {
            stateKey = try AES.GCM.open(
                box,
                using: SymmetricKey(data: secret),
                authenticating: aad
            )
        } catch {
            throw VTPMIdentityLifecycleError.invalidRecoveryPackage
        }
        guard stateKey.count == KeychainVTPMStateKeyStore.keyLength else {
            throw VTPMIdentityLifecycleError.invalidRecoveryPackage
        }
        try keyStore.replaceStateKey(stateKey, for: stableVMID)
    }

    func resetIdentity(
        stableVMID: String,
        stateDirectory: URL,
        now: Date = Date(),
        nonce: UUID = UUID()
    ) throws -> VTPMResetResult {
        let entries = (try? fileManager.contentsOfDirectory(atPath: stateDirectory.path)) ?? []
        let hasState = !entries.isEmpty
        let previousFingerprint = hasState ? try stateFingerprint(at: stateDirectory) : nil
        let suffix = "\(Int(now.timeIntervalSince1970))-\(nonce.uuidString.lowercased().prefix(8))"
        let archive = stateDirectory.deletingLastPathComponent()
            .appendingPathComponent("vtpm-archive-\(suffix)", isDirectory: true)
        let receipts = stateDirectory.deletingLastPathComponent()
            .appendingPathComponent("vtpm-lifecycle", isDirectory: true)
        let receiptURL = receipts.appendingPathComponent("reset-\(suffix).json")
        let oldKey: Data?
        if hasState {
            oldKey = try keyStore.stateKey(for: stableVMID, allowCreation: false)
        } else {
            oldKey = try? keyStore.stateKey(for: stableVMID, allowCreation: false)
        }
        let archivedKeyID = oldKey == nil ? nil : "\(stableVMID).archive.\(suffix)"

        if let oldKey, let archivedKeyID {
            try keyStore.replaceStateKey(oldKey, for: archivedKeyID)
        }
        var movedState = false
        do {
            if hasState {
                guard !fileManager.fileExists(atPath: archive.path) else {
                    throw CocoaError(.fileWriteFileExists)
                }
                try fileManager.moveItem(at: stateDirectory, to: archive)
                movedState = true
            } else if fileManager.fileExists(atPath: stateDirectory.path) {
                try fileManager.removeItem(at: stateDirectory)
            }
            try fileManager.createDirectory(
                at: stateDirectory,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            try fileManager.createDirectory(
                at: receipts,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            let receipt = VTPMLifecycleReceipt(
                format: Self.receiptFormat,
                operation: "reset",
                stableVMID: stableVMID,
                occurredAt: Self.timestamp(now),
                previousStateFingerprint: previousFingerprint,
                archivedStatePath: movedState ? archive.path : nil,
                archivedKeyID: archivedKeyID,
                recoveryPolicy: movedState
                    ? "encrypted state and device-local key archived; explicit recovery required"
                    : (archivedKeyID == nil
                        ? "no prior encrypted state or key"
                        : "orphan device-local key archived; no prior encrypted state"),
                sourceBundlePath: nil,
                destinationBundlePath: nil
            )
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
            try encoder.encode(receipt).write(to: receiptURL, options: [.atomic])
            try fileManager.setAttributes([.posixPermissions: 0o600], ofItemAtPath: receiptURL.path)
            try keyStore.deleteStateKey(for: stableVMID)
        } catch {
            do {
                try? fileManager.removeItem(at: receiptURL)
                if fileManager.fileExists(atPath: stateDirectory.path) {
                    try fileManager.removeItem(at: stateDirectory)
                }
                if movedState { try fileManager.moveItem(at: archive, to: stateDirectory) }
                if let oldKey { try keyStore.replaceStateKey(oldKey, for: stableVMID) }
                if let archivedKeyID { try? keyStore.deleteStateKey(for: archivedKeyID) }
            } catch {
                throw VTPMIdentityLifecycleError.resetRollbackFailed(error.localizedDescription)
            }
            throw error
        }
        return VTPMResetResult(
            archivedStatePath: movedState ? archive.path : nil,
            receiptPath: receiptURL.path
        )
    }

    /// Converts a copied VM bundle into a distinct TPM identity. The copied
    /// encrypted state is retained for audit/recovery, but the source key is
    /// never copied to the clone's stable ID.
    func prepareClonedIdentity(
        newStableVMID: String,
        copiedStateDirectory: URL,
        now: Date = Date(),
        nonce: UUID = UUID()
    ) throws -> VTPMResetResult {
        let entries = (try? fileManager.contentsOfDirectory(atPath: copiedStateDirectory.path)) ?? []
        let hasCopiedState = !entries.isEmpty
        let fingerprint = hasCopiedState ? try stateFingerprint(at: copiedStateDirectory) : nil
        let suffix = "\(Int(now.timeIntervalSince1970))-\(nonce.uuidString.lowercased().prefix(8))"
        let archive = copiedStateDirectory.deletingLastPathComponent()
            .appendingPathComponent("vtpm-source-copy-\(suffix)", isDirectory: true)
        let receipts = copiedStateDirectory.deletingLastPathComponent()
            .appendingPathComponent("vtpm-lifecycle", isDirectory: true)
        let receiptURL = receipts.appendingPathComponent("clone-reset-\(suffix).json")
        let orphanKey = try? keyStore.stateKey(for: newStableVMID, allowCreation: false)
        let orphanKeyID = orphanKey == nil ? nil : "\(newStableVMID).preclone.\(suffix)"
        if let orphanKey, let orphanKeyID {
            try keyStore.replaceStateKey(orphanKey, for: orphanKeyID)
        }
        var movedState = false
        do {
            if hasCopiedState {
                try fileManager.moveItem(at: copiedStateDirectory, to: archive)
                movedState = true
            } else if fileManager.fileExists(atPath: copiedStateDirectory.path) {
                try fileManager.removeItem(at: copiedStateDirectory)
            }
            try fileManager.createDirectory(
                at: copiedStateDirectory,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            try fileManager.createDirectory(
                at: receipts,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            let receipt = VTPMLifecycleReceipt(
                format: Self.receiptFormat,
                operation: "clone-reset",
                stableVMID: newStableVMID,
                occurredAt: Self.timestamp(now),
                previousStateFingerprint: fingerprint,
                archivedStatePath: movedState ? archive.path : nil,
                archivedKeyID: orphanKeyID,
                recoveryPolicy: "clone starts with a new TPM; source VM key is not duplicated",
                sourceBundlePath: nil,
                destinationBundlePath: nil
            )
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
            try encoder.encode(receipt).write(to: receiptURL, options: [.atomic])
            try fileManager.setAttributes([.posixPermissions: 0o600], ofItemAtPath: receiptURL.path)
            try keyStore.deleteStateKey(for: newStableVMID)
        } catch {
            try? fileManager.removeItem(at: receiptURL)
            try? fileManager.removeItem(at: copiedStateDirectory)
            if movedState { try? fileManager.moveItem(at: archive, to: copiedStateDirectory) }
            if let orphanKey { try? keyStore.replaceStateKey(orphanKey, for: newStableVMID) }
            if let orphanKeyID { try? keyStore.deleteStateKey(for: orphanKeyID) }
            throw error
        }
        return VTPMResetResult(
            archivedStatePath: movedState ? archive.path : nil,
            receiptPath: receiptURL.path
        )
    }

    func recordSameIdentityMove(
        stableVMID: String,
        stateDirectory: URL,
        sourceBundle: URL,
        destinationBundle: URL,
        now: Date = Date(),
        nonce: UUID = UUID()
    ) throws -> String {
        let entries = (try? fileManager.contentsOfDirectory(atPath: stateDirectory.path)) ?? []
        let fingerprint = entries.isEmpty ? nil : try stateFingerprint(at: stateDirectory)
        let suffix = "\(Int(now.timeIntervalSince1970))-\(nonce.uuidString.lowercased().prefix(8))"
        let receipts = stateDirectory.deletingLastPathComponent()
            .appendingPathComponent("vtpm-lifecycle", isDirectory: true)
        let receiptURL = receipts.appendingPathComponent("move-\(suffix).json")
        try fileManager.createDirectory(
            at: receipts,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        let receipt = VTPMLifecycleReceipt(
            format: Self.receiptFormat,
            operation: "move",
            stableVMID: stableVMID,
            occurredAt: Self.timestamp(now),
            previousStateFingerprint: fingerprint,
            archivedStatePath: nil,
            archivedKeyID: nil,
            recoveryPolicy: "same Mac and stable VM ID; Keychain identity retained unchanged",
            sourceBundlePath: sourceBundle.path,
            destinationBundlePath: destinationBundle.path
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        try encoder.encode(receipt).write(to: receiptURL, options: [.atomic])
        try fileManager.setAttributes([.posixPermissions: 0o600], ofItemAtPath: receiptURL.path)
        return receiptURL.path
    }

    func stateFingerprint(at directory: URL) throws -> String {
        var hasher = SHA256()
        hasher.update(data: Data("bridgevm-vtpm-state-v1\0".utf8))
        guard fileManager.fileExists(atPath: directory.path) else {
            return hasher.finalize().map { String(format: "%02x", $0) }.joined()
        }
        let rootValues = try directory.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        guard rootValues.isDirectory == true, rootValues.isSymbolicLink != true else {
            throw VTPMIdentityLifecycleError.unsafeStateEntry(directory.path)
        }
        let keys: Set<URLResourceKey> = [.isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey]
        guard let enumerator = fileManager.enumerator(
            at: directory,
            includingPropertiesForKeys: Array(keys),
            options: []
        ) else { throw CocoaError(.fileReadUnknown) }
        var files: [URL] = []
        for case let url as URL in enumerator {
            let values = try url.resourceValues(forKeys: keys)
            guard values.isSymbolicLink != true else {
                throw VTPMIdentityLifecycleError.unsafeStateEntry(url.path)
            }
            if values.isRegularFile == true { files.append(url) }
            else if values.isDirectory != true {
                throw VTPMIdentityLifecycleError.unsafeStateEntry(url.path)
            }
        }
        let rootPrefix = directory.standardizedFileURL.path + "/"
        for file in files.sorted(by: { $0.path < $1.path }) {
            let path = file.standardizedFileURL.path
            guard path.hasPrefix(rootPrefix) else {
                throw VTPMIdentityLifecycleError.unsafeStateEntry(path)
            }
            hasher.update(data: Data(path.dropFirst(rootPrefix.count).utf8))
            hasher.update(data: Data([0]))
            let handle = try FileHandle(forReadingFrom: file)
            defer { try? handle.close() }
            while let chunk = try handle.read(upToCount: 1024 * 1024), !chunk.isEmpty {
                hasher.update(data: chunk)
            }
            hasher.update(data: Data([0xff]))
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }

    private static func associatedData(
        stableVMID: String,
        createdAt: String,
        stateFingerprint: String
    ) -> Data {
        Data("\(recoveryFormat)|\(stableVMID)|\(createdAt)|\(stateFingerprint)".utf8)
    }

    private static func timestamp(_ date: Date) -> String {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter.string(from: date)
    }

    private static func recoveryCode(for data: Data) -> String {
        data.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }

    private static func decodeRecoveryCode(_ code: String) throws -> Data {
        var normalized = code.trimmingCharacters(in: .whitespacesAndNewlines)
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        while normalized.count % 4 != 0 { normalized.append("=") }
        guard let data = Data(base64Encoded: normalized),
              data.count == KeychainVTPMStateKeyStore.keyLength else {
            throw VTPMIdentityLifecycleError.invalidRecoveryCode
        }
        return data
    }
}
