import XCTest
@testable import BridgeVMControl

@MainActor
final class ControlModelLifecycleTests: XCTestCase {
    func testDuplicateStartWhileLaunchIsInFlightIsIgnored() async {
        let backend = LifecycleBackend(startVisibilityDelay: 0.1)
        let model = ControlModel(config: makeConfig(), backend: backend, startsAutomatically: false)

        model.start()
        model.start()
        await waitForLifecycle(model)

        XCTAssertEqual(backend.startCalls, 1)
    }

    func testStopDuringStartupWaitsForProcessThenStopsIt() async {
        let backend = LifecycleBackend(startVisibilityDelay: nil)
        let model = ControlModel(config: makeConfig(), backend: backend, startsAutomatically: false)
        model.start()
        await waitForLifecycle(model)
        XCTAssertTrue(model.running)
        let checksBeforeStop = backend.livenessChecks

        model.stop()
        await waitUntil { backend.livenessChecks > checksBeforeStop }; backend.setRunning(true)
        await waitForLifecycle(model, timeout: 4)

        XCTAssertEqual(backend.stopCalls, 1)
        XCTAssertFalse(backend.isRunning())
        XCTAssertFalse(model.running)
        XCTAssertEqual(model.statusNote, "정지됨")
    }

    func testInitialMissingProcessDuringStartConfirmationKeepsOptimisticState() async {
        let backend = LifecycleBackend(startVisibilityDelay: 0.3)
        let model = ControlModel(config: makeConfig(), backend: backend, startsAutomatically: false)
        model.start()
        await waitForLifecycle(model)
        let checksBeforeRefresh = backend.livenessChecks

        model.refreshStatus()
        await waitUntil { backend.livenessChecks > checksBeforeRefresh }

        XCTAssertTrue(model.running)
        XCTAssertEqual(model.statusNote, "VM 부팅 중…")
    }

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
        XCTAssertTrue(idle.model === model && idle.model.backend.supportsGuestCommands)
        XCTAssertTrue(idle.model.terminalLog.contains("Enter a command when the guest is ready."))
        XCTAssertNil(LibraryDashboardRuntimeState.stopped.note(model.statusNote))
        XCTAssertEqual(model.ip, "—")
    }

    private func waitForLifecycle(_ model: ControlModel, timeout: TimeInterval = 1) async {
        let deadline = Date().addingTimeInterval(timeout)
        while model.lifecycleBusy && Date() < deadline {
            try? await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTAssertFalse(model.lifecycleBusy)
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

private final class LifecycleBackend: VMBackend {
    let displayName = "Lifecycle Test"
    let kind = "test"
    let supportsGuestCommands = false
    let supportsPackageInstall = false
    let supportsClipboard = false
    let supportsSSH = false
    let supportsResourceChanges = false

    private let lock = NSLock()
    private let startVisibilityDelay: TimeInterval?
    private var processRunning = false
    private var _startCalls = 0
    private var _stopCalls = 0
    private var _livenessChecks = 0

    init(startVisibilityDelay: TimeInterval?) { self.startVisibilityDelay = startVisibilityDelay }

    var startCalls: Int { lock.withLock { _startCalls } }
    var stopCalls: Int { lock.withLock { _stopCalls } }
    var livenessChecks: Int { lock.withLock { _livenessChecks } }
    func isRunning() -> Bool { lock.withLock { _livenessChecks += 1; return processRunning } }
    func setRunning(_ value: Bool) { lock.withLock { processRunning = value } }
    func currentIP() -> String? { nil }
    func start() -> Bool {
        lock.withLock { _startCalls += 1 }
        guard let startVisibilityDelay else { return true }
        DispatchQueue.global().asyncAfter(deadline: .now() + startVisibilityDelay) { [weak self] in
            self?.lock.withLock { self?.processRunning = true }
        }
        return true
    }
    func stop() { lock.withLock { _stopCalls += 1; processRunning = false } }
    func resources() -> (memMiB: Int, cpu: Int) { (4096, 2) }
    func setResources(memMiB: Int, cpu: Int) -> Bool { false }
    func runInGuest(_ command: String) -> (output: String, code: Int32) { ("", -1) }
}
