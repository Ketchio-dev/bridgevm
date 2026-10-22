import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class ResourceRegistrationFixture {
    let root: URL, libraryRoot: URL, registration: URL, launch: URL, handoff: URL
    let original: VMConfig
    let backend: ResourceRegistrationBackend
    let library: LibraryModel
    let model: ControlModel
    private var keepForPendingWork = false

    init(compensate: Bool = false, validHandoff: Bool = true) throws {
        let temporary = try XCTUnwrap(ProcessInfo.processInfo.environment["TMPDIR"])
        let ownedRoot = URL(fileURLWithPath: temporary).resolvingSymlinksInPath()
            .appendingPathComponent("resource-registration-" + UUID().uuidString)
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
        let bytes = try JSONSerialization.data(withJSONObject: ["resources": ["memory": "4096", "cpu": "2"]])
        try bytes.write(to: launchURL, options: .withoutOverwriting)
        try (validHandoff ? bytes : Data("{}".utf8)).write(to: handoffURL, options: .withoutOverwriting)
        let config = VMConfig(id: "active", name: "Active", displayName: "Active", backendKind: "fast-vz",
            bootMode: "direct-kernel", bundlePath: metadata.deletingLastPathComponent().path,
            runnerPath: ownedRoot.appendingPathComponent("absent-runner").path,
            launchSpecPath: launchURL.path, handoffPath: handoffURL.path,
            sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "owned",
            displayWidth: 1280, displayHeight: 720, memMiB: 4096, cpuCount: 2)
        guard VMLibrary.save(config, rootURL: ownedLibrary) else { throw CocoaError(.fileWriteUnknown) }
        let saved = try NativeLibraryReader.readConfig(rootURL: ownedLibrary, id: config.slug)
        original = saved
        registration = ownedLibrary.appendingPathComponent(config.slug + "/vm.json")
        let backend = ResourceRegistrationBackend(saved.makeBackend(libraryRoot: ownedLibrary), compensate: compensate)
        self.backend = backend
        library = LibraryModel(rootURL: ownedLibrary, migrateLegacy: false, startsModelsAutomatically: false,
            modelFactory: { ControlModel(config: $0, backend: backend, startsAutomatically: false) })
        model = library.model(for: saved)
        model.applyRuntimeStatus(running: true, ip: nil) // Synthetic observed state, no OS process.
        prepared = true
    }

    deinit { if !keepForPendingWork { try? FileManager.default.removeItem(at: root) } }

    func beginApply(expectSaved: Bool = true) async throws -> VMConfig {
        model.pendingMemGiB = 8; model.pendingCPU = 4
        keepForPendingWork = true
        model.applyResources()
        guard await backend.waitForSaved() else { throw CocoaError(.fileWriteUnknown) }
        XCTAssertTrue(model.busy)
        XCTAssertEqual(model.config, original, "MainActor acknowledgment has not run")
        let saved = try NativeLibraryReader.readConfig(rootURL: libraryRoot, id: original.slug)
        XCTAssertEqual(saved.memMiB, expectSaved ? 8192 : 4096); XCTAssertEqual(saved.cpuCount, expectSaved ? 4 : 2)
        return saved
    }

    func finishApply() async throws {
        backend.release()
        try await wait { !self.model.busy && !self.model.lifecycleBusy }
        keepForPendingWork = false
        XCTAssertFalse(backend.gateTimedOut)
        XCTAssertEqual(backend.counts, [1, 1, 1], "Actual Apply uses only simulated restart methods")
        XCTAssertTrue(backend.isRunning())
    }

    func finishFailedApply() async throws {
        backend.release()
        try await wait { !self.model.busy && !self.model.lifecycleBusy }
        keepForPendingWork = false
        XCTAssertFalse(backend.gateTimedOut)
        XCTAssertEqual(backend.counts, [1, 0, 0], "A failed writer must not restart")
        XCTAssertEqual(model.statusNote, "리소스 적용 실패 — VM은 재시작하지 않았습니다.")
    }

    func adoptOther() throws {
        var other = original
        other.id = "other"; other.name = "Other"; other.displayName = "Other"
        other.bundlePath = root.appendingPathComponent("other-bundle").path
        other.launchSpecPath = root.appendingPathComponent("other-launch").path
        other.handoffPath = root.appendingPathComponent("other-handoff").path
        guard VMLibrary.save(other, rootURL: libraryRoot) else { throw CocoaError(.fileWriteUnknown) }
        var creation = CreateVMCreationState()
        let result = creation.complete(.created(other), adopt: library.add)
        XCTAssertTrue(result.dismiss, "Actual creation completion adopts already persisted registration")
        XCTAssertEqual(library.selectedID, other.slug)
    }

    func ordinaryControls() throws -> LibraryDashboardLegacyControls {
        library.selectedID = original.slug
        let dashboard = try XCTUnwrap(values(LibraryDashboardDetail.self,
            in: LibraryDashboardRoute(library: library).body).first)
        return try XCTUnwrap(values(LibraryDashboardLegacyControls.self, in: dashboard.body).first)
    }

    func stopThroughDashboard(_ controls: LibraryDashboardLegacyControls) async throws {
        controls.model.refreshStatus()
        try await wait { controls.model.running }
        let action = try XCTUnwrap(values(DashboardActionButton.self, in: controls.body)
            .first { $0.title == "정지" })
        XCTAssertFalse(action.disabled)
        action.action()
        try await wait { !controls.model.lifecycleBusy }
    }

    func metadataBytes() throws -> [Data] { try [launch, handoff].map { try Data(contentsOf: $0) } }

    private func wait(_ condition: () -> Bool) async throws {
        let deadline = ProcessInfo.processInfo.systemUptime + 8
        while !condition(), ProcessInfo.processInfo.systemUptime < deadline {
            try await Task.sleep(nanoseconds: 5_000_000)
        }
        guard condition() else { throw CocoaError(.fileReadUnknown) }
    }

    private func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let result = value as? T { return [result] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }
}
