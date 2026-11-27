import XCTest
@testable import BridgeVMControl

@MainActor
final class ControlModelLifecycleTests: XCTestCase {
    func testObservedWindowsStatusAfterRegistrationChangesEngine() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        var config = makeConfig()
        config.backendKind = "hvf-engine"
        config.bootMode = "windows-hvf"
        config.bundlePath = root.appendingPathComponent("fixture.vmbridge").path
        let marker = root.appendingPathComponent("observed.raw").path
        config.diskPath = marker
        XCTAssertTrue(VMLibrary.save(config, rootURL: root))
        let library = LibraryModel(rootURL: root, migrateLegacy: false, startsModelsAutomatically: false)
        library.selectedID = config.slug
        let model = library.model(for: config)

        let child = Process(), input = Pipe()
        child.executableURL = URL(fileURLWithPath: "/bin/sh")
        child.arguments = ["-c", "read -r line", marker]
        child.standardInput = input
        try child.run()
        defer { if child.isRunning { child.terminate(); child.waitUntilExit() } }
        model.refreshStatus()
        await waitUntil(timeout: 3) { model.running }
        XCTAssertTrue(model.running)
        XCTAssertNil(LibraryDashboardRuntimeState.running.note(model.statusNote))

        var replacement = config
        replacement.backendKind = "legacy"
        XCTAssertTrue(VMLibrary.save(replacement, rootURL: root))
        library.reload()
        let selected = try XCTUnwrap(library.selectedDetail)
        XCTAssertTrue(selected.model === model)
        XCTAssertNil(library.hvfRuntimeDetailSession(for: selected.config))
        let dashboard = LibraryDashboardDetail(config: selected.config, model: selected.model, library: library)
        XCTAssertEqual(dashboard.visibleStatusNote, "Running")

        try input.fileHandleForWriting.close()
        child.waitUntilExit()
        model.refreshStatus()
        await waitUntil(timeout: 3) { !model.running }
        XCTAssertFalse(model.running)
        XCTAssertEqual(dashboard.visibleStatusNote, "VM exited")
        let idle = try XCTUnwrap(library.selectedDetail)
        XCTAssertEqual(idle.config.engineKind, .unsupported)
        XCTAssertTrue(idle.model === model)
        XCTAssertNil(LibraryDashboardRuntimeState.stopped.note(model.statusNote))
    }

    private func waitUntil(timeout: TimeInterval = 1, _ condition: @escaping () -> Bool) async {
        let deadline = Date().addingTimeInterval(timeout)
        while !condition() && Date() < deadline {
            try? await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTAssertTrue(condition())
    }

    private func makeConfig() -> VMConfig {
        VMConfig(id: "lifecycle-test", name: "Lifecycle Test", displayName: "Lifecycle Test",
                 backendKind: "fast-vz", bootMode: nil, bundlePath: "", runnerPath: "",
                 launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                 leasesPath: "", guestName: "", displayWidth: 1, displayHeight: 1)
    }
}
