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
            ownedRuntime: true)
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

@MainActor
final class HvfOwnedStartExecution {
    struct Input: @unchecked Sendable {
        let config: HvfEngineConfig
        let repoRoot: URL
        let keyProvider: VTPMExistingStateKeyReading?
        let launch: (Process) throws -> Void
        let deadline: TimeInterval
        let now: () -> TimeInterval
        let effectAdmission: HvfRuntimeEffectAdmission
        let permit: @MainActor () -> Bool
        func checkClock() throws {
            guard now() < deadline else { throw HvfOwnedStartFailure.deadlineExceeded }
            guard effectAdmission.isValid else { throw HvfOwnedStartFailure.configurationChanged }
        }
    }
    typealias Worker = (Input) async -> Result<HvfOwnedRuntimeSpawn, HvfOwnedStartFailure>
    private let session: HvfEngineSession
    private let ticket: HvfOwnedStartOperation
    private let revalidate: @MainActor () -> Bool
    private var task: Task<Void, Never>?
    init(session: HvfEngineSession, ticket: HvfOwnedStartOperation, revalidate: @escaping @MainActor () -> Bool) {
        self.session = session; self.ticket = ticket; self.revalidate = revalidate
    }
    func begin() {
        let session = session, ticket = ticket, revalidate = revalidate
        ticket.watchDeadline { [weak session] in
            session?.refreshOwnedStart(); session?.objectWillChange.send()
        }
        let input = Input(config: ticket.configuration, repoRoot: session.repoRoot,
            keyProvider: session.vtpmKeyProvider as? VTPMExistingStateKeyReading, launch: session.processLaunch,
            deadline: ticket.deadlineUptime, now: session.ownedStartNow, effectAdmission: ticket.effectAdmission,
            permit: { session.admitOwnedStartEffect(ticket, revalidate: revalidate) })
        let worker = session.ownedStartWorker
        task = Task {
            let result = await Task.detached { await worker(input) }.value
            session.finishOwnedStartWorker(ticket, result: result, revalidate: revalidate)
        }
    }
    nonisolated static func run(_ input: Input) async -> Result<HvfOwnedRuntimeSpawn, HvfOwnedStartFailure> {
        var key: Data?
        defer { let count = key?.count ?? 0; key?.resetBytes(in: 0..<count) }
        do {
            guard await input.permit() else { throw HvfOwnedStartFailure.configurationChanged }
            try input.checkClock()
            let runner = input.repoRoot.appendingPathComponent("target/release/hvf-runner")
            guard FileManager.default.isExecutableFile(atPath: runner.path) else { throw HvfOwnedStartFailure.helperUnavailable }
            guard input.config.readiness(repoRoot: input.repoRoot).launchReady else { throw HvfOwnedStartFailure.readiness }
            if input.config.vtpmStateDir != nil {
                guard await input.permit() else { throw HvfOwnedStartFailure.configurationChanged }
                try input.checkClock()
                guard let provider = input.keyProvider else { throw HvfOwnedStartFailure.keyUnavailable }
                do { key = try VTPMExistingRuntimeKey.withExistingKey(for: input.config, provider: provider) { $0 } }
                catch { throw HvfOwnedStartFailure.keyUnavailable }
            }
            guard await input.permit() else { throw HvfOwnedStartFailure.configurationChanged }
            try input.checkClock()
            let manifest: Data
            do { manifest = try HvfRuntimePreparation.prepare(config: input.config) }
            catch { throw HvfOwnedStartFailure.preparation }
            guard await input.permit() else { throw HvfOwnedStartFailure.configurationChanged }
            try input.checkClock()
            do {
                return .success(try HvfOwnedRuntimeLaunch.spawn(config: input.config, repoRoot: input.repoRoot,
                    runner: runner, manifest: manifest, key: key, launch: { process in
                        try input.checkClock(); try input.launch(process)
                    }))
            } catch let failure as HvfOwnedStartFailure { throw failure }
            catch { throw HvfOwnedStartFailure.processLaunch }
        } catch let failure as HvfOwnedStartFailure { return .failure(failure) }
        catch { return .failure(.preparation) }
    }
}
