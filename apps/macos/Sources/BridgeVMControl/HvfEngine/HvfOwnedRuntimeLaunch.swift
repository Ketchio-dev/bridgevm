import Foundation
import CryptoKit

enum HvfOwnedRuntimeLaunch {
    @MainActor
    static func start(config: HvfEngineConfig, repoRoot: URL, runner: URL, manifest: Data,
                      keyProvider: VTPMStateKeyProviding, launch: (Process) throws -> Void)
        throws -> (controller: HvfOwnedRunController, helloFrame: Data) {
        let token = UUID()
        let digest = SHA256.hash(data: manifest).map { String(format: "%02x", $0) }.joined()
        var hello = try VTPMRuntimeKey.withKey(for: config, provider: keyProvider) { key in
            try HvfOwnedRuntimeCodec.frame(HvfOwnedRuntimeHello(schemaVersion: 1, kind: "hello",
                runToken: token.uuidString.lowercased(), sequence: 0, manifestSHA256: digest,
                keyHex: key.map { $0.map { String(format: "%02x", $0) }.joined() }))
        }
        var transferred = false
        defer { if !transferred { hello.resetBytes(in: hello.indices) } }
        let channel = try HvfOwnedRuntimeChannel()
        let guestShutdown = HvfOwnedGuestShutdown(path: config.ctlFilePath)
        let process = Process()
        process.executableURL = runner
        let firmware = repoRoot.appendingPathComponent("firmware/edk2-aarch64-secure-code.fd")
        let fallback = repoRoot.appendingPathComponent("crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd")
        let arguments = config.runnerArguments(manifestPath: config.evidenceDir + "/launch-manifest.json",
            runnerPath: runner.path, firmwareCodePath: FileManager.default.fileExists(atPath: firmware.path)
                ? firmware.path : fallback.path,
            probePath: repoRoot.appendingPathComponent("target/release/examples/hvf_gic_boot_probe").path,
            ownedRuntime: true)
        process.arguments = Array(arguments.dropFirst())
        process.currentDirectoryURL = repoRoot
        process.environment = ProcessInfo.processInfo.environment.filter { !$0.key.hasPrefix("BRIDGEVM_") }
        channel.attach(to: process)
        do { try launch(process) }
        catch { channel.close(); guestShutdown.close(); throw error }
        let identity = HvfOwnedRuntimeIdentity(token: token, processID: process.processIdentifier)
        let controller = HvfOwnedRunController(identity: identity, config: config, process: process,
            manifestSHA256: digest, channel: channel, guestShutdown: guestShutdown)
        transferred = true
        return (controller, hello)
    }
}
