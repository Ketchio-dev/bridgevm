import Foundation
import XCTest
@testable import BridgeVMControl

/// A real private app-owner route without App.main, WindowServer, or a real guest.
@MainActor
final class HvfOwnedRunnerCLIFixture {
    let runner: HvfOwnedRunnerFixture
    let model: LibraryModel
    let owner: NativeRuntimeOwner
    let router: NativeRuntimeAppControlRouter
    let appInstanceID = UUID().uuidString
    let config: VMConfig
    private var cli: Process?

    init(runner: HvfOwnedRunnerFixture) throws {
        self.runner = runner
        let root = runner.base.root.appendingPathComponent("library")
        config = VMConfig(id: "owned-fixture", name: "owned-fixture", displayName: "Owned fixture",
            backendKind: "hvf-engine", bootMode: "windows-hvf",
            bundlePath: root.appendingPathComponent("owned-fixture/bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: "fixture", displayWidth: 1280, displayHeight: 720, installPending: false)
        guard VMLibrary.save(config, rootURL: root) else { throw CocoaError(.fileWriteUnknown) }
        model = LibraryModel(rootURL: root, migrateLegacy: false,
            runtimeSessionFactory: { _ in runner.session }, startsModelsAutomatically: false)
        let library = try NativeRuntimeLibraryHandle.open(rootURL: root, create: false)
        owner = try NativeRuntimeOwner(library: library)
        router = NativeRuntimeAppControlRouter(appInstanceID: appInstanceID, library: library.identity,
            validateOwner: { [owner] in try owner.validateCurrentOwnership() }, retainedModel: { [model] in model })
        try owner.start(controlHandler: { [router] request, context in
            try await router.handle(request, context: context)
        }, handler: { [model, appInstanceID] request in
            try await NativeRuntimeAppObservation.response(request, library: library.identity,
                appInstanceID: appInstanceID, retainedModel: model)
        })
        XCTAssertTrue(model.hvfRuntimeSession(for: config) === runner.session)
    }

    func stop(throughPairedRustCLI: Bool = false) async throws -> (Int32, [String: Any]) {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["BRIDGEVM_TEST_NATIVE_CLI"])
        guard path.hasPrefix("/"), FileManager.default.isExecutableFile(atPath: path) else {
            throw CocoaError(.executableNotLoadable)
        }
        let outputURL = runner.base.root.appendingPathComponent("cli-" + UUID().uuidString + ".json")
        guard FileManager.default.createFile(atPath: outputURL.path, contents: nil) else {
            throw CocoaError(.fileWriteUnknown)
        }
        let output = try FileHandle(forWritingTo: outputURL)
        defer { try? output.close() }
        let process = Process()
        process.executableURL = try throughPairedRustCLI ? pairedCLI(nativePath: path) : URL(fileURLWithPath: path)
        process.arguments = [throughPairedRustCLI ? "app" : "--cli", "stop", config.slug,
                             "--library", model.rootURL.path, "--json"]
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = output
        process.standardError = output
        try process.run()
        cli = process
        let deadline = ProcessInfo.processInfo.systemUptime + 10
        while process.isRunning && ProcessInfo.processInfo.systemUptime < deadline {
            runner.session.poll()
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        guard !process.isRunning else { throw CocoaError(.executableRuntimeMismatch) }
        let data = try Data(contentsOf: outputURL)
        XCTAssertTrue(data.count <= 65_536, "CLI output must stay within its 64 KiB response bound")
        let value = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        return (process.terminationStatus, value)
    }

    private func pairedCLI(nativePath: String) throws -> URL {
        let rustPath = try XCTUnwrap(ProcessInfo.processInfo.environment["BRIDGEVM_TEST_RUST_CLI"])
        guard rustPath.hasPrefix("/"), FileManager.default.isExecutableFile(atPath: rustPath) else {
            throw CocoaError(.executableNotLoadable)
        }
        let contents = runner.base.root.appendingPathComponent("Owned Fixture.app/Contents")
        let cli = contents.appendingPathComponent("Resources/target/release/bridgevm")
        let native = contents.appendingPathComponent("MacOS/BridgeVMControl")
        for directory in [cli.deletingLastPathComponent(), native.deletingLastPathComponent()] {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        }
        try FileManager.default.copyItem(atPath: rustPath, toPath: cli.path)
        try FileManager.default.copyItem(atPath: nativePath, toPath: native.path)
        return cli
    }

    func close() async {
        if let cli, cli.isRunning {
            cli.terminate()
            let deadline = ProcessInfo.processInfo.systemUptime + 2
            while cli.isRunning && ProcessInfo.processInfo.systemUptime < deadline {
                try? await Task.sleep(nanoseconds: 10_000_000)
            }
        }
        owner.close()
        XCTAssertFalse(cli?.isRunning == true, "Retained CLI process must exit before fixture removal")
        if cli?.isRunning != true { await runner.clean() }
    }
}
