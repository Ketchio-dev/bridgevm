import Foundation
import Security

/// Headless lifecycle surface carried by the packaged BridgeVMControl binary.
/// The default no-argument path remains the SwiftUI app. Every command here is
/// explicit, closed-set, and keeps the device-local vTPM state key off argv,
/// environment variables, stdout, and disk.
enum VTPMLifecycleCommand {
    enum CommandError: LocalizedError {
        case usage(String)
        case invalidPath(String)
        case existingOutput(String)
        case missingInput(String)
        case processLaunch(String)

        var errorDescription: String? {
            switch self {
            case let .usage(message): return message
            case let .invalidPath(path): return "invalid path: \(path)"
            case let .existingOutput(path): return "refusing to overwrite existing output: \(path)"
            case let .missingInput(path): return "required input is missing: \(path)"
            case let .processLaunch(message): return "packaged HVF launch failed: \(message)"
            }
        }
    }

    private struct Options {
        var values: [String: String] = [:]
        var flags: Set<String> = []
    }

    static let usage = """
    usage: BridgeVMControl --vtpm-lifecycle COMMAND OPTIONS

      export      --stable-vm-id ID --state-dir DIR --package FILE --recovery-code-file FILE
      restore     --stable-vm-id ID --state-dir DIR --package FILE --recovery-code-file FILE
      clone-reset --new-stable-vm-id ID --state-dir DIR
      run         --stable-vm-id ID --state-dir DIR --target FILE --vars FILE --evidence-dir DIR
                  [--ram-mib N] [--smp-cpus N] [--max-exits N] [--firmware-code FILE] [--no-network]
                  [--virtio-gpu-3d] [--gpu-device-id 1050|10f7] [--performance-risk balanced|aggressive]
                  [--require-real-title-gate]
                  [--ephemeral-recovery --package FILE --recovery-code-file FILE]

    Recovery-code files are exclusively created with mode 0600. Key custody
    always uses the Data Protection Keychain with ThisDeviceOnly.
    The run command uses only the packaged scripts, firmware, probe, CLI, and swtpm runtime.
    """

    static func run(arguments: [String]) -> Int32 {
        do {
            guard let command = arguments.first else { throw CommandError.usage(usage) }
            if command == "help" || command == "--help" || command == "-h" {
                print(usage)
                return 0
            }
            let options = try parse(Array(arguments.dropFirst()))
            switch command {
            case "export": try exportRecovery(options)
            case "restore": try restoreRecovery(options)
            case "clone-reset": try cloneReset(options)
            case "run": return try runPackagedVM(options)
            default: throw CommandError.usage("unknown lifecycle command: \(command)\n\n\(usage)")
            }
            return 0
        } catch {
            FileHandle.standardError.write(Data("ERROR: \(error.localizedDescription)\n".utf8))
            return 1
        }
    }

    private static func parse(_ arguments: [String]) throws -> Options {
        let valueOptions: Set<String> = [
            "--stable-vm-id", "--new-stable-vm-id", "--state-dir", "--package",
            "--recovery-code-file", "--target", "--vars", "--evidence-dir",
            "--ram-mib", "--smp-cpus", "--max-exits", "--firmware-code", "--gpu-device-id",
            "--performance-risk",
        ]
        let flagOptions: Set<String> = [
            "--no-network", "--virtio-gpu-3d", "--require-real-title-gate", "--ephemeral-recovery",
        ]
        var result = Options()
        var index = 0
        while index < arguments.count {
            let option = arguments[index]
            if flagOptions.contains(option) {
                guard result.flags.insert(option).inserted else {
                    throw CommandError.usage("duplicate option: \(option)")
                }
                index += 1
                continue
            }
            guard valueOptions.contains(option), index + 1 < arguments.count else {
                throw CommandError.usage("unknown or incomplete option: \(option)\n\n\(usage)")
            }
            guard result.values[option] == nil else {
                throw CommandError.usage("duplicate option: \(option)")
            }
            result.values[option] = arguments[index + 1]
            index += 2
        }
        return result
    }

    private static func require(_ option: String, _ options: Options) throws -> String {
        guard let value = options.values[option]?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else { throw CommandError.usage("missing required option: \(option)") }
        return value
    }

    private static func fileURL(_ raw: String, directory: Bool = false) throws -> URL {
        let expanded = (raw as NSString).expandingTildeInPath
        guard expanded.hasPrefix("/") else { throw CommandError.invalidPath(raw) }
        return URL(fileURLWithPath: expanded, isDirectory: directory).standardizedFileURL
    }

    private static func exportRecovery(_ options: Options) throws {
        let stableID = try require("--stable-vm-id", options)
        let state = try fileURL(require("--state-dir", options), directory: true)
        let package = try fileURL(require("--package", options))
        let codeFile = try fileURL(require("--recovery-code-file", options))
        for output in [package, codeFile] where FileManager.default.fileExists(atPath: output.path) {
            throw CommandError.existingOutput(output.path)
        }
        guard FileManager.default.fileExists(atPath: state.path) else {
            throw CommandError.missingInput(state.path)
        }
        let result = try VTPMIdentityLifecycle(keyStore: keyStore())
            .exportRecovery(stableVMID: stableID, stateDirectory: state, destination: package)
        try VTPMStateSecurity.createPrivateFile(Data((result.recoveryCode + "\n").utf8), at: codeFile)
        print("exported_package=\(package.path)")
        print("recovery_code_file=\(codeFile.path)")
        print("state_fingerprint=\(result.stateFingerprint)")
    }

    private static func restoreRecovery(_ options: Options) throws {
        let stableID = try require("--stable-vm-id", options)
        let state = try fileURL(require("--state-dir", options), directory: true)
        let package = try fileURL(require("--package", options))
        let codeFile = try fileURL(require("--recovery-code-file", options))
        for input in [state, package, codeFile] where !FileManager.default.fileExists(atPath: input.path) {
            throw CommandError.missingInput(input.path)
        }
        let code = try String(contentsOf: codeFile, encoding: .utf8)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        try VTPMIdentityLifecycle(keyStore: keyStore()).restoreRecovery(
            stableVMID: stableID,
            stateDirectory: state,
            packageURL: package,
            recoveryCode: code
        )
        print("restored_stable_vm_id=\(stableID)")
    }

    private static func cloneReset(_ options: Options) throws {
        let stableID = try require("--new-stable-vm-id", options)
        let state = try fileURL(require("--state-dir", options), directory: true)
        let result = try VTPMIdentityLifecycle(keyStore: keyStore())
            .prepareClonedIdentity(newStableVMID: stableID, copiedStateDirectory: state)
        print("clone_stable_vm_id=\(stableID)")
        print("archived_state=\(result.archivedStatePath ?? "none")")
        print("receipt=\(result.receiptPath)")
    }

    private static func runPackagedVM(_ options: Options) throws -> Int32 {
        let stableID = try require("--stable-vm-id", options)
        let state = try fileURL(require("--state-dir", options), directory: true)
        let target = try fileURL(require("--target", options))
        let vars = try fileURL(require("--vars", options))
        let evidence = try fileURL(require("--evidence-dir", options), directory: true)
        for input in [target, vars] where !FileManager.default.isReadableFile(atPath: input.path) {
            throw CommandError.missingInput(input.path)
        }
        try FileManager.default.createDirectory(
            at: state, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        try FileManager.default.createDirectory(
            at: evidence, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        let resources = try packagedResourcesRoot()
        let wrapper = resources.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh")
        let probe = resources.appendingPathComponent("target/release/examples/hvf_gic_boot_probe")
        let cli = resources.appendingPathComponent("target/release/bridgevm")
        let swtpm = Bundle.main.bundleURL.appendingPathComponent("Contents/Helpers/swtpm")
        let defaultFirmware = resources.appendingPathComponent("firmware/edk2-aarch64-secure-code.fd")
        let firmware = try options.values["--firmware-code"].map { try fileURL($0) } ?? defaultFirmware
        for executable in [wrapper, probe, cli, swtpm]
        where !FileManager.default.isExecutableFile(atPath: executable.path) {
            throw CommandError.missingInput(executable.path)
        }
        guard FileManager.default.isReadableFile(atPath: firmware.path) else {
            throw CommandError.missingInput(firmware.path)
        }
        let ram = try positiveInt(options.values["--ram-mib"] ?? "6144", name: "--ram-mib")
        let cpus = try positiveInt(options.values["--smp-cpus"] ?? "4", name: "--smp-cpus")
        let maxExits = try positiveInt(
            options.values["--max-exits"] ?? "50000000", name: "--max-exits")
        let ctl = evidence.appendingPathComponent("agent-control.txt")
        let input = evidence.appendingPathComponent("input.ctl")
        FileManager.default.createFile(atPath: ctl.path, contents: Data())
        FileManager.default.createFile(atPath: input.path, contents: Data())

        let existingState = try VTPMStateSecurity.stateDirectoryContainsData(at: state.path)
        let ephemeral = options.flags.contains("--ephemeral-recovery")
        let store: VTPMStateKeyManaging = ephemeral ? MemoryVTPMStateKeyStore() : keyStore()
        let recoveryPackage = try options.values["--package"].map { try fileURL($0) }
        let recoveryCodeFile = try options.values["--recovery-code-file"].map { try fileURL($0) }
        if ephemeral {
            guard let recoveryPackage, let recoveryCodeFile else {
                throw CommandError.usage("--ephemeral-recovery requires --package and --recovery-code-file")
            }
            if existingState {
                for input in [recoveryPackage, recoveryCodeFile]
                where !FileManager.default.fileExists(atPath: input.path) {
                    throw CommandError.missingInput(input.path)
                }
                let code = try String(contentsOf: recoveryCodeFile, encoding: .utf8)
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                try VTPMIdentityLifecycle(keyStore: store).restoreRecovery(
                    stableVMID: stableID,
                    stateDirectory: state,
                    packageURL: recoveryPackage,
                    recoveryCode: code)
            } else {
                for output in [recoveryPackage, recoveryCodeFile]
                where FileManager.default.fileExists(atPath: output.path) {
                    throw CommandError.existingOutput(output.path)
                }
            }
        }
        let key = try store.stateKey(for: stableID, allowCreation: !existingState)
        let keyInput = try VTPMProcessKeyInput(key: key)
        var args = [
            wrapper.path,
            "--target", target.path,
            "--vars", vars.path,
            "--firmware-code", firmware.path,
            "--evidence-dir", evidence.path,
            "--ram-mib", String(ram),
            "--smp-cpus", String(cpus),
            "--max-exits", String(maxExits),
            "--release", "--skip-build", "--no-watchdog",
            "--agent-service-control", ctl.path,
            "--agent-service-command", "whoami",
            "--boot-timer", "--boot-timer-desktop-agent",
            "--display-export-ppm", evidence.appendingPathComponent("display.ppm").path,
            "--display-export-ms", "100",
            "--display-export-fb", evidence.appendingPathComponent("display.fb").path,
            "--enable-xhci", "--input-control", input.path,
            "--vtpm-state-dir", state.path,
            "--swtpm-bin", swtpm.path,
            "--swtpm-key-stdin",
        ]
        if !options.flags.contains("--no-network") { args.append("--virtio-net") }
        if options.flags.contains("--require-real-title-gate"),
           !options.flags.contains("--virtio-gpu-3d") {
            throw CommandError.usage("--require-real-title-gate requires --virtio-gpu-3d")
        }
        if options.flags.contains("--virtio-gpu-3d") {
            let deviceID = (options.values["--gpu-device-id"] ?? "10f7").lowercased()
            guard deviceID == "1050" || deviceID == "10f7" else {
                throw CommandError.usage("--gpu-device-id must be 1050 or 10f7")
            }
            let risk = options.values["--performance-risk"] ?? "aggressive"
            guard risk == "balanced" || risk == "aggressive" else {
                throw CommandError.usage("--performance-risk must be balanced or aggressive")
            }
            args.append(contentsOf: [
                "--performance-risk", risk,
                "--virtio-gpu-3d",
                "--virtio-gpu-device-id", deviceID,
                "--gpu-trace", evidence.appendingPathComponent("virtio-gpu.jsonl").path,
                "--gpu-trace-protocol", "venus",
            ])
            if options.flags.contains("--require-real-title-gate") {
                args.append("--require-real-title-gate")
            }
        }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = args
        process.currentDirectoryURL = resources
        process.environment = ProcessInfo.processInfo.environment.filter { !$0.key.hasPrefix("BRIDGEVM_") }
        keyInput.attach(to: process)
        do {
            try process.run()
            try keyInput.deliverAfterLaunch()
        } catch {
            keyInput.discard()
            process.terminate()
            throw CommandError.processLaunch(error.localizedDescription)
        }
        print("packaged_run_pid=\(process.processIdentifier)")
        print("agent_control=\(ctl.path)")
        print("evidence_dir=\(evidence.path)")
        process.waitUntilExit()
        let status = process.terminationStatus
        if ephemeral, !existingState, status == 0,
           let recoveryPackage, let recoveryCodeFile {
            let result = try VTPMIdentityLifecycle(keyStore: store).exportRecovery(
                stableVMID: stableID,
                stateDirectory: state,
                destination: recoveryPackage)
            try VTPMStateSecurity.createPrivateFile(
                Data((result.recoveryCode + "\n").utf8), at: recoveryCodeFile)
            print("exported_package=\(recoveryPackage.path)")
            print("recovery_code_file=\(recoveryCodeFile.path)")
            print("state_fingerprint=\(result.stateFingerprint)")
        }
        return status
    }

    private final class MemoryVTPMStateKeyStore: VTPMStateKeyManaging {
        private var keys: [String: Data] = [:]

        func stateKey(for stableVMID: String, allowCreation: Bool) throws -> Data {
            let account = stableVMID.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !account.isEmpty, account.utf8.count <= 255 else {
                throw VTPMStateSecurityError.invalidVMIdentifier
            }
            if let key = keys[account] { return key }
            guard allowCreation else { throw VTPMStateSecurityError.missingKeyForExistingState }
            let keyLength = KeychainVTPMStateKeyStore.keyLength
            var key = Data(count: keyLength)
            let status = key.withUnsafeMutableBytes {
                SecRandomCopyBytes(kSecRandomDefault, keyLength, $0.baseAddress!)
            }
            guard status == errSecSuccess else {
                key.resetBytes(in: key.indices)
                throw VTPMStateSecurityError.randomGeneration(status)
            }
            keys[account] = key
            return key
        }

        func replaceStateKey(_ key: Data, for stableVMID: String) throws {
            guard key.count == KeychainVTPMStateKeyStore.keyLength else {
                throw VTPMStateSecurityError.invalidKeyLength(key.count)
            }
            keys[stableVMID] = key
        }

        func deleteStateKey(for stableVMID: String) throws {
            if var key = keys.removeValue(forKey: stableVMID) {
                key.resetBytes(in: key.indices)
            }
        }

        deinit {
            for name in Array(keys.keys) { try? deleteStateKey(for: name) }
        }
    }

    private static func keyStore() -> KeychainVTPMStateKeyStore {
        KeychainVTPMStateKeyStore()
    }

    private static func positiveInt(_ raw: String, name: String) throws -> Int {
        guard let value = Int(raw), value > 0 else {
            throw CommandError.usage("\(name) must be a positive integer")
        }
        return value
    }

    private static func packagedResourcesRoot() throws -> URL {
        guard let resource = Bundle.main.resourceURL else {
            throw CommandError.missingInput("packaged Resources directory")
        }
        return resource
    }
}
