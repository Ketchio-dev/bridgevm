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
        let backend = LifecycleBackend(startVisibilityDelay: 0.2)
        let model = ControlModel(config: makeConfig(), backend: backend, startsAutomatically: false)
        model.start()
        await waitForLifecycle(model)
        XCTAssertTrue(model.running)
        XCTAssertFalse(backend.isRunning())

        model.stop()
        await waitForLifecycle(model, timeout: 4)

        XCTAssertEqual(backend.stopCalls, 1)
        XCTAssertFalse(backend.isRunning())
        XCTAssertFalse(model.running)
        XCTAssertEqual(model.statusNote, "정지됨")
    }

    private func waitForLifecycle(_ model: ControlModel, timeout: TimeInterval = 1) async {
        let deadline = Date().addingTimeInterval(timeout)
        while model.lifecycleBusy && Date() < deadline {
            try? await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTAssertFalse(model.lifecycleBusy)
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
    private let startVisibilityDelay: TimeInterval
    private var processRunning = false
    private var _startCalls = 0
    private var _stopCalls = 0

    init(startVisibilityDelay: TimeInterval) {
        self.startVisibilityDelay = startVisibilityDelay
    }

    var startCalls: Int { lock.withLock { _startCalls } }
    var stopCalls: Int { lock.withLock { _stopCalls } }
    func isRunning() -> Bool { lock.withLock { processRunning } }
    func currentIP() -> String? { nil }
    func start() -> Bool {
        lock.withLock { _startCalls += 1 }
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
