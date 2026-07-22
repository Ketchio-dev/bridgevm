import Foundation
import Security

enum VTPMStateSecurityError: LocalizedError, Equatable {
    case invalidVMIdentifier
    case invalidKeyLength(Int)
    case missingKeyForExistingState
    case randomGeneration(OSStatus)
    case keychainRead(OSStatus)
    case keychainWrite(OSStatus)

    var errorDescription: String? {
        switch self {
        case .invalidVMIdentifier:
            return "vTPM 키에 사용할 안정적인 VM ID가 없습니다."
        case let .invalidKeyLength(length):
            return "vTPM 상태 키 길이가 32바이트가 아닙니다 (현재 \(length)바이트)."
        case .missingKeyForExistingState:
            return "기존 vTPM 상태의 Keychain 키가 없습니다. 상태를 덮어쓰지 않았습니다. 복구 키를 복원하거나 명시적으로 TPM을 재설정하세요."
        case let .randomGeneration(status):
            return "vTPM 상태 키 난수 생성에 실패했습니다 (OSStatus \(status))."
        case let .keychainRead(status):
            return "Keychain에서 vTPM 상태 키를 읽지 못했습니다 (OSStatus \(status))."
        case let .keychainWrite(status):
            return "Keychain에 vTPM 상태 키를 저장하지 못했습니다 (OSStatus \(status))."
        }
    }
}

protocol VTPMStateKeyProviding {
    /// Returns the stable 256-bit key for one VM, creating it atomically on the
    /// first launch. Implementations must never serialize the key into vm.json.
    func stateKey(for stableVMID: String, allowCreation: Bool) throws -> Data
}

protocol VTPMStateKeyManaging: VTPMStateKeyProviding {
    func replaceStateKey(_ key: Data, for stableVMID: String) throws
    func deleteStateKey(for stableVMID: String) throws
}

/// Device-local vTPM key custody. `ThisDeviceOnly` deliberately prevents an
/// encrypted TPM state directory from becoming silently usable after a bundle
/// copy to another Mac. The recovery flow makes that trust transition explicit
/// with a separately keyed authenticated package instead of weakening this
/// storage class.
final class KeychainVTPMStateKeyStore: VTPMStateKeyManaging {
    static let service = "com.bridgevm.vtpm-state-key.v1"
    static let keyLength = 32

    func stateKey(for stableVMID: String, allowCreation: Bool) throws -> Data {
        let account = try validatedAccount(stableVMID)

        let existing = read(account: account)
        if existing.status == errSecSuccess, let key = existing.data {
            guard key.count == Self.keyLength else {
                throw VTPMStateSecurityError.invalidKeyLength(key.count)
            }
            return key
        }
        guard existing.status == errSecItemNotFound else {
            throw VTPMStateSecurityError.keychainRead(existing.status)
        }
        guard allowCreation else {
            throw VTPMStateSecurityError.missingKeyForExistingState
        }

        var generated = Data(count: Self.keyLength)
        let randomStatus = generated.withUnsafeMutableBytes { buffer in
            SecRandomCopyBytes(kSecRandomDefault, Self.keyLength, buffer.baseAddress!)
        }
        guard randomStatus == errSecSuccess else {
            generated.resetBytes(in: generated.indices)
            throw VTPMStateSecurityError.randomGeneration(randomStatus)
        }

        let addStatus = SecItemAdd([
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: Self.service,
            kSecAttrAccount: account,
            kSecAttrAccessible: kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
            kSecUseDataProtectionKeychain: true,
            kSecValueData: generated,
        ] as CFDictionary, nil)
        if addStatus == errSecSuccess { return generated }

        // Two app entry points may race on the first launch. The winner's key
        // is authoritative; discard ours and re-read rather than overwriting.
        generated.resetBytes(in: generated.indices)
        if addStatus == errSecDuplicateItem {
            let winner = read(account: account)
            guard winner.status == errSecSuccess, let key = winner.data else {
                throw VTPMStateSecurityError.keychainRead(winner.status)
            }
            guard key.count == Self.keyLength else {
                throw VTPMStateSecurityError.invalidKeyLength(key.count)
            }
            return key
        }
        throw VTPMStateSecurityError.keychainWrite(addStatus)
    }

    func replaceStateKey(_ key: Data, for stableVMID: String) throws {
        guard key.count == Self.keyLength else {
            throw VTPMStateSecurityError.invalidKeyLength(key.count)
        }
        let account = try validatedAccount(stableVMID)
        let query = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: Self.service,
            kSecAttrAccount: account,
            kSecUseDataProtectionKeychain: true,
        ] as CFDictionary
        let updateStatus = SecItemUpdate(query, [kSecValueData: key] as CFDictionary)
        if updateStatus == errSecSuccess { return }
        guard updateStatus == errSecItemNotFound else {
            throw VTPMStateSecurityError.keychainWrite(updateStatus)
        }
        let addStatus = SecItemAdd([
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: Self.service,
            kSecAttrAccount: account,
            kSecAttrAccessible: kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
            kSecUseDataProtectionKeychain: true,
            kSecValueData: key,
        ] as CFDictionary, nil)
        guard addStatus == errSecSuccess else {
            throw VTPMStateSecurityError.keychainWrite(addStatus)
        }
    }

    func deleteStateKey(for stableVMID: String) throws {
        let account = try validatedAccount(stableVMID)
        let status = SecItemDelete([
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: Self.service,
            kSecAttrAccount: account,
            kSecUseDataProtectionKeychain: true,
        ] as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw VTPMStateSecurityError.keychainWrite(status)
        }
    }

    private func validatedAccount(_ stableVMID: String) throws -> String {
        let account = stableVMID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !account.isEmpty, account.utf8.count <= 255,
              !account.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) })
        else { throw VTPMStateSecurityError.invalidVMIdentifier }
        return account
    }

    private func read(account: String) -> (data: Data?, status: OSStatus) {
        var item: CFTypeRef?
        let status = SecItemCopyMatching([
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: Self.service,
            kSecAttrAccount: account,
            kSecUseDataProtectionKeychain: true,
            kSecReturnData: true,
            kSecMatchLimit: kSecMatchLimitOne,
        ] as CFDictionary, &item)
        guard status == errSecSuccess else { return (nil, status) }
        guard let data = item as? Data else { return (nil, errSecDecode) }
        return (data, errSecSuccess)
    }
}

/// One-shot inherited-FD transport. The wrapper/swtpm contract consumes the
/// key from stdin and closes it before launching the HVF probe.
final class VTPMProcessKeyInput {
    private let pipe = Pipe()
    private var key: Data

    init(key: Data) throws {
        guard key.count == KeychainVTPMStateKeyStore.keyLength else {
            throw VTPMStateSecurityError.invalidKeyLength(key.count)
        }
        self.key = key
    }

    func attach(to process: Process) {
        process.standardInput = pipe
    }

    func deliverAfterLaunch() throws {
        try? pipe.fileHandleForReading.close()
        defer {
            try? pipe.fileHandleForWriting.close()
            key.resetBytes(in: key.indices)
        }
        try pipe.fileHandleForWriting.write(contentsOf: key)
    }

    func discard() {
        try? pipe.fileHandleForReading.close()
        try? pipe.fileHandleForWriting.close()
        key.resetBytes(in: key.indices)
    }

    deinit { discard() }
}

enum VTPMStateSecurity {
    static func processInput(
        for config: HvfEngineConfig,
        provider: VTPMStateKeyProviding,
        fileManager: FileManager = .default
    ) throws -> VTPMProcessKeyInput? {
        guard let stateDir = config.vtpmStateDir else { return nil }
        guard let keyID = config.vtpmKeyID else {
            throw VTPMStateSecurityError.invalidVMIdentifier
        }
        let existingState = ((try? fileManager.contentsOfDirectory(atPath: stateDir)) ?? [])
            .contains { !$0.isEmpty }
        return try VTPMProcessKeyInput(key: provider.stateKey(
            for: keyID,
            allowCreation: !existingState
        ))
    }

    static func defaultSwtpmCommand(
        fileManager: FileManager = .default,
        environment: [String: String] = ProcessInfo.processInfo.environment,
        bundle: Bundle = .main
    ) -> String {
        if let override = environment["BRIDGEVM_SWTPM_BIN"], !override.isEmpty {
            return override
        }
        let conventionalHelper = bundle.bundleURL
            .appendingPathComponent("Contents/Helpers/swtpm", isDirectory: false)
        for bundled in [bundle.url(forAuxiliaryExecutable: "swtpm"), conventionalHelper]
            .compactMap({ $0 }) where fileManager.isExecutableFile(atPath: bundled.path) {
                return bundled.path
        }
        for candidate in ["/opt/homebrew/bin/swtpm", "/usr/local/bin/swtpm", "/usr/bin/swtpm"]
        where fileManager.isExecutableFile(atPath: candidate) {
            return candidate
        }
        return "swtpm"
    }

    static func executableAvailable(
        _ command: String,
        fileManager: FileManager = .default,
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> Bool {
        if command.contains("/") { return fileManager.isExecutableFile(atPath: command) }
        return (environment["PATH"] ?? "")
            .split(separator: ":", omittingEmptySubsequences: true)
            .map(String.init)
            .contains { directory in
                fileManager.isExecutableFile(atPath: URL(fileURLWithPath: directory)
                    .appendingPathComponent(command).path)
            }
    }
}
