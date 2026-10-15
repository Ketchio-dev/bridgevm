import Combine
import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class ResourceDisplayCompletionFixture {
    let root: URL, libraryRoot: URL, launch: URL, handoff: URL, registration: URL
    let original: VMConfig
    let originalBytes: [Data]
    let backend: ResourceDisplayCompletionBackend
    let library: LibraryModel
    let model: ControlModel
    private var keepForPendingWork = false

    init(validHandoff: Bool = true) throws {
        let temporary = try XCTUnwrap(ProcessInfo.processInfo.environment["TMPDIR"])
        let ownedRoot = URL(fileURLWithPath: temporary).resolvingSymlinksInPath()
            .appendingPathComponent("resource-display-" + UUID().uuidString)
        root = ownedRoot
        guard mkdir(ownedRoot.path, 0o700) == 0 else { throw CocoaError(.fileWriteFileExists) }
        var prepared = false
        defer { if !prepared { try? FileManager.default.removeItem(at: ownedRoot) } }
        let ownedLibrary = ownedRoot.appendingPathComponent("library")
        libraryRoot = ownedLibrary
        let metadata = ownedRoot.appendingPathComponent("bundle/metadata")
        try FileManager.default.createDirectory(at: metadata, withIntermediateDirectories: true)
        let launchURL = metadata.appendingPathComponent("apple-vz-launch.json")
        let handoffURL = metadata.appendingPathComponent("handoff.json")
        launch = launchURL; handoff = handoffURL
        let resources = ["resources": ["memory": "4096", "cpu": "2"]]
        try JSONSerialization.data(withJSONObject: resources).write(to: launchURL, options: .withoutOverwriting)
        try JSONSerialization.data(withJSONObject: validHandoff ? resources : [:])
            .write(to: handoffURL, options: .withoutOverwriting)
        let config = VMConfig(id: ownedRoot.lastPathComponent, name: "Owned resources", displayName: "Owned resources",
            backendKind: "fast-vz", bootMode: "direct-kernel", bundlePath: metadata.deletingLastPathComponent().path,
            runnerPath: ownedRoot.appendingPathComponent("absent-runner").path, launchSpecPath: launchURL.path,
            handoffPath: handoffURL.path, sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "owned",
            displayWidth: 1280, displayHeight: 720, memMiB: 4096, cpuCount: 2)
        guard VMLibrary.save(config, rootURL: ownedLibrary) else { throw CocoaError(.fileWriteUnknown) }
        let saved = try NativeLibraryReader.readConfig(rootURL: ownedLibrary, id: config.slug)
        original = saved
        let registrationURL = ownedLibrary.appendingPathComponent(config.slug + "/vm.json")
        registration = registrationURL
        originalBytes = try [registrationURL, launchURL, handoffURL].map { try Data(contentsOf: $0) }
        let backend = ResourceDisplayCompletionBackend(saved.makeBackend(libraryRoot: ownedLibrary))
        self.backend = backend
        let ownedModelLibrary = LibraryModel(rootURL: ownedLibrary, migrateLegacy: false, startsModelsAutomatically: false,
            modelFactory: { ControlModel(config: $0, backend: backend, startsAutomatically: false) })
        library = ownedModelLibrary
        model = ownedModelLibrary.model(for: saved)
        prepared = true
    }

    deinit { if !keepForPendingWork { try? FileManager.default.removeItem(at: root) } }

    func initializeDisplay() async throws {
        model.refresh()
        try await waitUntil { self.model.memGiB == 4 && self.model.cpu == 2 }
        XCTAssertEqual(model.config, original)
        XCTAssertEqual(backend.processEffects, 0)
    }

    func apply(memoryGiB: Double = 8, cpu: Double = 4, editDraft: Bool = false) async throws -> ResourceDisplaySnapshot {
        var completion: ResourceDisplaySnapshot?
        // @Published emits synchronously before assignment. Read the resource fields,
        // not lifecycleBusy itself, at its false transition, before any refresh burst.
        let observer = model.$lifecycleBusy.dropFirst().sink { [weak model] busy in
            guard !busy, let model else { return }
            completion = ResourceDisplaySnapshot(model)
        }
        defer { observer.cancel(); backend.release() }
        model.pendingMemGiB = memoryGiB
        model.pendingCPU = cpu
        keepForPendingWork = true
        model.applyResources()
        guard await backend.waitForSaved() else {
            keepForPendingWork = model.busy || model.lifecycleBusy
            XCTFail("Actual writer did not acknowledge within 3 seconds")
            throw CocoaError(.fileWriteUnknown)
        }
        XCTAssertTrue(model.busy)
        if editDraft { model.pendingMemGiB = 16; model.pendingCPU = 8 }
        backend.release()
        try await waitUntil { !self.model.busy && !self.model.lifecycleBusy }
        keepForPendingWork = false
        XCTAssertFalse(backend.gateTimedOut)
        XCTAssertEqual(backend.writeCalls, 1)
        XCTAssertEqual(backend.processEffects, 0)
        return try XCTUnwrap(completion, "Completion observation was not delivered")
    }

    func assertSaved(memoryMiB: Int, cpu: Int) throws {
        let saved = try NativeLibraryReader.readConfig(rootURL: libraryRoot, id: original.slug)
        XCTAssertEqual(saved.memMiB, memoryMiB)
        XCTAssertEqual(saved.cpuCount, cpu)
        XCTAssertEqual(model.config, saved)
        XCTAssertEqual(library.latestConfiguration(for: original), saved)
        for url in [launch, handoff] {
            let object = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
            let resources = try XCTUnwrap(object["resources"] as? [String: String])
            XCTAssertEqual(resources["memory"], String(memoryMiB))
            XCTAssertEqual(resources["cpu"], String(cpu))
        }
    }

    private func waitUntil(_ condition: () -> Bool) async throws {
        let deadline = ProcessInfo.processInfo.systemUptime + 3
        while !condition(), ProcessInfo.processInfo.systemUptime < deadline {
            try await Task.sleep(nanoseconds: 5_000_000)
        }
        guard condition() else { XCTFail("Model did not acknowledge within 3 seconds"); throw CocoaError(.fileReadUnknown) }
    }
}

@MainActor
struct ResourceDisplaySnapshot {
    let memoryGiB: Double, cpu: Int, pendingMemoryGiB: Double, pendingCPU: Double
    let config: VMConfig
    let refreshGeneration: UInt64
    init(_ model: ControlModel) {
        memoryGiB = model.memGiB; cpu = model.cpu; config = model.config
        pendingMemoryGiB = model.pendingMemGiB; pendingCPU = model.pendingCPU
        refreshGeneration = model.refreshGeneration
    }
}
