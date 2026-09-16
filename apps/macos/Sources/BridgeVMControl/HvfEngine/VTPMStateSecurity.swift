import Darwin
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
    static func createPrivateFile(_ data: Data, at url: URL) throws {
        let descriptor = try PrivateStateFileDescriptor.create(at: url)
        var succeeded = false
        defer {
            if !succeeded { _ = Darwin.ftruncate(descriptor, 0) }
            Darwin.close(descriptor)
        }
        try data.withUnsafeBytes { rawBuffer in
            guard var address = rawBuffer.baseAddress else { return }
            var remaining = rawBuffer.count
            while remaining > 0 {
                let written = Darwin.write(descriptor, address, remaining)
                if written < 0 {
                    if errno == EINTR { continue }
                    throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno))
                }
                guard written > 0 else {
                    throw NSError(domain: NSPOSIXErrorDomain, code: Int(EIO))
                }
                remaining -= written
                address = address.advanced(by: written)
            }
        }
        guard Darwin.fsync(descriptor) == 0 else {
            throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno))
        }
        succeeded = true
    }

    static func stateDirectoryContainsData(
        at path: String,
        fileManager: FileManager = .default
    ) throws -> Bool {
        var isDirectory: ObjCBool = false
        guard fileManager.fileExists(atPath: path, isDirectory: &isDirectory) else {
            return false
        }
        guard isDirectory.boolValue else {
            throw CocoaError(.fileReadUnknown)
        }
        return try fileManager.contentsOfDirectory(atPath: path).contains { !$0.isEmpty }
    }

    static func processInput(
        for config: HvfEngineConfig,
        provider: VTPMStateKeyProviding,
        fileManager: FileManager = .default
    ) throws -> VTPMProcessKeyInput? {
        try VTPMRuntimeKey.withKey(for: config, provider: provider, fileManager: fileManager) { key in
            try key.map { try VTPMProcessKeyInput(key: $0) }
        }
    }

    static func defaultSwtpmCommand(
        fileManager: FileManager = .default,
        environment: [String: String] = ProcessInfo.processInfo.environment,
        bundle: Bundle = .main
    ) -> String {
        // DEBUG only: swtpm holds the vTPM's sealed state, so choosing which
        // binary plays that role by environment variable is a way to hand it
        // to something else entirely.
        #if DEBUG
        if let override = environment["BRIDGEVM_SWTPM_BIN"], !override.isEmpty {
            return override
        }
        #endif
        let conventionalHelper = bundle.bundleURL
            .appendingPathComponent("Contents/Helpers/swtpm", isDirectory: false)
        for bundled in [bundle.url(forAuxiliaryExecutable: "swtpm"), conventionalHelper]
            .compactMap({ $0 }) where fileManager.isExecutableFile(atPath: bundled.path) {
                return bundled.path
        }
        // DEBUG only: a release build runs the helper it shipped and signed, or
        // none. Falling back to whatever is on PATH means an attacker who can
        // write to /usr/local/bin chooses the TPM.
        #if DEBUG
        for candidate in ["/opt/homebrew/bin/swtpm", "/usr/local/bin/swtpm", "/usr/bin/swtpm"]
        where fileManager.isExecutableFile(atPath: candidate) {
            return candidate
        }
        return "swtpm"
        #else
        return ""
        #endif
    }

    static func executableAvailable(
        _ command: String,
        fileManager: FileManager = .default,
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> Bool {
        // An empty command is how a release build reports "no bundled helper".
        // It must fail closed here rather than fall through to a PATH search.
        if command.isEmpty { return false }
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
