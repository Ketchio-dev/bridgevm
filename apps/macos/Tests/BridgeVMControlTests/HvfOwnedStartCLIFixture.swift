import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfOwnedStartCLIFixture {
    let runner: HvfOwnedStartRunnerFixture
    let model: LibraryModel
    let library: NativeRuntimeLibraryHandle
    let owner: NativeRuntimeOwner
    let handlers: NativeRuntimeAppRequestHandlers
    let appInstanceID = UUID().uuidString
    private var cli: Process?
    private var paired: URL?

    init(runner: HvfOwnedStartRunnerFixture) throws {
        self.runner = runner
        model = LibraryModel(rootURL: runner.libraryRoot, migrateLegacy: false,
            runtimeSessionFactory: { runner.makeSession($0) }, startsModelsAutomatically: false)
        library = try NativeRuntimeLibraryHandle.open(rootURL: runner.libraryRoot, create: false)
        owner = try NativeRuntimeOwner(library: library)
        handlers = NativeRuntimeAppRequestHandlers(appInstanceID: appInstanceID, library: library.identity,
            validateOwner: { [owner] in try owner.validateCurrentOwnership() }, retainedModel: { [model] in model })
        try handlers.serve(owner: owner) { [model, appInstanceID, library] request in
            try await NativeRuntimeAppObservation.response(request, library: library.identity,
                appInstanceID: appInstanceID, retainedModel: model)
        }
    }

    func command(_ verb: String, pairedRust: Bool) async throws -> (Int32, [String: Any]) {
        let native = try XCTUnwrap(ProcessInfo.processInfo.environment["BRIDGEVM_TEST_NATIVE_CLI"])
        guard native.hasPrefix("/"), FileManager.default.isExecutableFile(atPath: native) else { throw CocoaError(.executableNotLoadable) }
        let outputURL = runner.root.appendingPathComponent("cli-" + UUID().uuidString + ".json")
        guard FileManager.default.createFile(atPath: outputURL.path, contents: nil) else { throw CocoaError(.fileWriteUnknown) }
        let output = try FileHandle(forWritingTo: outputURL)
        defer { try? output.close() }
        let process = Process()
        process.executableURL = try pairedRust ? pairedCLI(native: native) : URL(fileURLWithPath: native)
        process.arguments = [pairedRust ? "app" : "--cli", verb, runner.config.slug, "--library", model.rootURL.path, "--json"]
        process.standardInput = FileHandle.nullDevice; process.standardOutput = output; process.standardError = output
        try process.run(); cli = process
        let deadline = ProcessInfo.processInfo.systemUptime + 10
        while process.isRunning, ProcessInfo.processInfo.systemUptime < deadline {
            try runner.adoptChildren(); runner.session?.poll()
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        guard !process.isRunning else { throw CocoaError(.executableRuntimeMismatch) }
        let bytes = try Data(contentsOf: outputURL)
        XCTAssertTrue(bytes.count <= 65_536, "CLI response must stay within its 64 KiB bound")
        return (process.terminationStatus, try XCTUnwrap(JSONSerialization.jsonObject(with: bytes) as? [String: Any]))
    }

    func startStatus(operationID: String) async throws -> NativeRuntimeStartResponse {
        let request = NativeRuntimeStartRequest(schema: NativeRuntimeStartCodec.requestSchema, operation: .startStatus,
            requestID: UUID().uuidString, library: library.identity, vmID: runner.config.slug, appInstanceID: appInstanceID,
            expectedSavedConfigurationDigest: try NativeRuntimeConfigurationIdentity.digest(config: runner.config), operationID: operationID)
        let root = runner.libraryRoot
        return try await Task.detached { try NativeRuntimeStartClient.query(rootURL: root, request: request) }.value
    }

    private func pairedCLI(native: String) throws -> URL {
        if let paired { return paired }
        let rust = try XCTUnwrap(ProcessInfo.processInfo.environment["BRIDGEVM_TEST_RUST_CLI"])
        guard rust.hasPrefix("/"), FileManager.default.isExecutableFile(atPath: rust) else { throw CocoaError(.executableNotLoadable) }
        let contents = runner.root.appendingPathComponent("Owned Start Fixture.app/Contents")
        let cli = contents.appendingPathComponent("Resources/target/release/bridgevm")
        let nativeURL = contents.appendingPathComponent("MacOS/BridgeVMControl")
        for directory in [cli.deletingLastPathComponent(), nativeURL.deletingLastPathComponent()] {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        }
        try FileManager.default.copyItem(atPath: rust, toPath: cli.path)
        try FileManager.default.copyItem(atPath: native, toPath: nativeURL.path)
        paired = cli
        return cli
    }

    func close() async {
        if let cli, cli.isRunning {
            cli.terminate()
            let deadline = ProcessInfo.processInfo.systemUptime + 2
            while cli.isRunning, ProcessInfo.processInfo.systemUptime < deadline { try? await Task.sleep(nanoseconds: 10_000_000) }
        }
        owner.close()
        XCTAssertFalse(cli?.isRunning == true, "Exact CLI exit must precede fixture removal")
        if cli?.isRunning != true { await runner.clean() }
    }
}
