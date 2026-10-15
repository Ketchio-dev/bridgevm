import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class ControlModelResourceCompletionTests: XCTestCase {
    func testEditingDraftDuringApplyDoesNotMisreportSavedAllocation() async throws {
        try await checkCompletion(editDraft: true)
    }

    func testUnchangedDraftReportsAcceptedAllocation() async throws {
        try await checkCompletion(editDraft: false)
    }

    private func checkCompletion(editDraft: Bool) async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("resource-completion-" + UUID().uuidString)
        var keepForPendingWork = false
        defer { if !keepForPendingWork { try? FileManager.default.removeItem(at: root) } }
        let metadata = root.appendingPathComponent("bundle/metadata")
        try FileManager.default.createDirectory(at: metadata, withIntermediateDirectories: true)
        let launch = metadata.appendingPathComponent("apple-vz-launch.json")
        let handoff = metadata.appendingPathComponent("handoff.json")
        let initial = try JSONSerialization.data(withJSONObject: ["resources": ["memory": "4096", "cpu": "2"]])
        try initial.write(to: launch)
        try initial.write(to: handoff)
        let config = VMConfig(id: root.lastPathComponent, name: "Owned resources", displayName: "Owned resources",
            backendKind: "fast-vz", bootMode: "direct-kernel", bundlePath: metadata.deletingLastPathComponent().path,
            runnerPath: root.appendingPathComponent("absent-runner").path, launchSpecPath: launch.path,
            handoffPath: handoff.path, sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "owned",
            displayWidth: 1280, displayHeight: 720, memMiB: 4096, cpuCount: 2)
        XCTAssertTrue(VMLibrary.save(config, rootURL: root))
        let saved = try NativeLibraryReader.readConfig(rootURL: root, id: config.slug)
        let backend = ResourceCompletionBackend(saved.makeBackend(libraryRoot: root))
        defer { backend.release() }
        let library = LibraryModel(rootURL: root, migrateLegacy: false, modelFactory: { config in
            ControlModel(config: config, backend: backend, startsAutomatically: false)
        })
        let model = library.model(for: saved)
        model.pendingMemGiB = 8
        model.pendingCPU = 4
        keepForPendingWork = true
        model.applyResources()
        guard await backend.waitForSaved() else {
            keepForPendingWork = model.busy
            return XCTFail("Actual resource writer did not reach its acknowledgment boundary")
        }
        XCTAssertTrue(model.busy)
        // These two sliders remain editable while the Apply button is disabled.
        if editDraft { model.pendingMemGiB = 16; model.pendingCPU = 8 }
        backend.release()
        let deadline = ProcessInfo.processInfo.systemUptime + 3
        while model.busy, ProcessInfo.processInfo.systemUptime < deadline {
            try await Task.sleep(nanoseconds: 5_000_000)
        }
        guard !model.busy else {
            keepForPendingWork = true
            return XCTFail("Accepted resource work did not acknowledge completion")
        }
        keepForPendingWork = false
        XCTAssertFalse(backend.gateTimedOut)
        let persisted = try NativeLibraryReader.readConfig(rootURL: root, id: saved.slug)
        XCTAssertEqual(persisted.memMiB, 8192)
        XCTAssertEqual(persisted.cpuCount, 4)
        XCTAssertEqual(backend.resources().memMiB, 8192)
        for url in [launch, handoff] {
            let object = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
            let resources = try XCTUnwrap(object["resources"] as? [String: String])
            XCTAssertEqual(resources["memory"], "8192")
            XCTAssertEqual(resources["cpu"], "4")
        }
        XCTAssertEqual(library.latestConfiguration(for: saved), persisted)
        XCTAssertEqual(model.statusNote, "리소스 적용됨: 8GB / 4코어")
        XCTAssertEqual(model.pendingMemGiB, editDraft ? 16 : 8)
        XCTAssertEqual(model.pendingCPU, editDraft ? 8 : 4)
    }
}

/// Keeps the real Fast VZ resource writer, while preventing process effects.
private final class ResourceCompletionBackend: VMBackend {
    let displayName = "Owned resources", kind = "owned"
    let supportsGuestCommands = false, supportsPackageInstall = false
    let supportsClipboard = false, supportsSSH = false, supportsResourceChanges = true
    private let wrapped: VMBackend
    private let saved = DispatchSemaphore(value: 0), finish = DispatchSemaphore(value: 0)
    private let lock = NSLock()
    private var timedOut = false
    init(_ wrapped: VMBackend) { self.wrapped = wrapped }
    var gateTimedOut: Bool { lock.lock(); defer { lock.unlock() }; return timedOut }
    func release() { finish.signal() }
    func waitForSaved() async -> Bool {
        await Task.detached { [saved] in saved.wait(timeout: .now() + 3) == .success }.value
    }
    func setResources(memMiB: Int, cpu: Int) -> Bool {
        let result = wrapped.setResources(memMiB: memMiB, cpu: cpu)
        saved.signal()
        if finish.wait(timeout: .now() + 5) == .timedOut { lock.lock(); timedOut = true; lock.unlock() }
        return result
    }
    func resources() -> (memMiB: Int, cpu: Int) { wrapped.resources() }
    func isRunning() -> Bool { false }
    func currentIP() -> String? { nil }
    func start() -> Bool { XCTFail("The resource test must not launch a VM"); return false }
    func stop() { XCTFail("The resource test must not stop a VM") }
    func runInGuest(_ command: String) -> (output: String, code: Int32) {
        XCTFail("The resource test must not invoke a guest"); return ("", -1)
    }
}
