import Foundation
import CryptoKit

final class HvfOwnedRuntimeSpawn: @unchecked Sendable {
    let process: Process
    let channel: HvfOwnedRuntimeChannel
    let guestShutdown: HvfOwnedGuestShutdown
    let identity: HvfOwnedRuntimeIdentity
    let config: HvfEngineConfig
    let digest: String
    var hello: Data
    init(process: Process, channel: HvfOwnedRuntimeChannel, guestShutdown: HvfOwnedGuestShutdown,
         token: UUID, config: HvfEngineConfig, digest: String, hello: Data) {
        self.process = process; self.channel = channel; self.guestShutdown = guestShutdown
        identity = .init(token: token, processID: process.processIdentifier)
        self.config = config; self.digest = digest; self.hello = hello
    }
    deinit { hello.resetBytes(in: hello.indices) }
}

extension HvfOwnedRuntimeLaunch {
    static func spawn(config: HvfEngineConfig, repoRoot: URL, runner: URL, manifest: Data,
                      key: Data?, launch: (Process) throws -> Void) throws -> HvfOwnedRuntimeSpawn {
        let token = UUID()
        let digest = SHA256.hash(data: manifest).map { String(format: "%02x", $0) }.joined()
        var hello = try HvfOwnedRuntimeCodec.frame(HvfOwnedRuntimeHello(schemaVersion: 1, kind: "hello",
            runToken: token.uuidString.lowercased(), sequence: 0, manifestSHA256: digest,
            keyHex: key.map { $0.map { String(format: "%02x", $0) }.joined() }))
        defer { hello.resetBytes(in: hello.indices) }
        let channel = try HvfOwnedRuntimeChannel()
        let guestShutdown = HvfOwnedGuestShutdown(path: config.ctlFilePath)
        let process = Process(); process.executableURL = runner
        let firmware = repoRoot.appendingPathComponent("firmware/edk2-aarch64-secure-code.fd")
        let fallback = repoRoot.appendingPathComponent("crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd")
        let arguments = config.runnerArguments(manifestPath: config.evidenceDir + "/launch-manifest.json",
            runnerPath: runner.path, firmwareCodePath: FileManager.default.fileExists(atPath: firmware.path)
                ? firmware.path : fallback.path,
            probePath: repoRoot.appendingPathComponent("target/release/examples/hvf_gic_boot_probe").path,
            ownedRuntime: true, agentAssetsDir: repoRoot.appendingPathComponent("scripts/win-assets").path)
        process.arguments = Array(arguments.dropFirst()); process.currentDirectoryURL = repoRoot
        process.environment = ProcessInfo.processInfo.environment.filter { !$0.key.hasPrefix("BRIDGEVM_") }
        channel.attach(to: process)
        do { try launch(process) }
        catch { channel.close(); guestShutdown.close(); throw error }
        return HvfOwnedRuntimeSpawn(process: process, channel: channel, guestShutdown: guestShutdown,
            token: token, config: config, digest: digest, hello: hello)
    }
    @MainActor
    static func adopt(_ spawn: HvfOwnedRuntimeSpawn) -> (controller: HvfOwnedRunController, helloFrame: Data) {
        let controller = HvfOwnedRunController(identity: spawn.identity, config: spawn.config,
            process: spawn.process, manifestSHA256: spawn.digest, channel: spawn.channel, guestShutdown: spawn.guestShutdown)
        return (controller, spawn.hello)
    }
}
