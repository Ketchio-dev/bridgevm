import XCTest
@testable import BridgeVMControl

@MainActor
final class ControlModelResourceTests: XCTestCase {
    func testFailedResourceSaveNeverChecksLivenessOrRestartsVM() async {
        let backend = ResourceBackend(supportsChanges: true, setResult: false, running: true)
        let model = ControlModel(config: makeConfig(), backend: backend, startsAutomatically: false)
        model.pendingMemGiB = 6
        model.pendingCPU = 3

        model.applyResources()
        await waitUntilIdle(model)

        XCTAssertEqual(backend.setCalls, 1)
        XCTAssertEqual(backend.livenessChecks, 0)
        XCTAssertEqual(backend.stopCalls, 0)
        XCTAssertEqual(backend.startCalls, 0)
        XCTAssertTrue(model.statusNote.contains("재시작하지 않았습니다"))
    }

    func testUnsupportedBackendRejectsResourceChangeSynchronously() {
        let backend = ResourceBackend(supportsChanges: false, setResult: true, running: true)
        let model = ControlModel(config: makeConfig(), backend: backend, startsAutomatically: false)

        model.applyResources()

        XCTAssertFalse(model.busy)
        XCTAssertEqual(backend.setCalls, 0)
        XCTAssertEqual(backend.stopCalls, 0)
        XCTAssertTrue(model.statusNote.contains("지원하지 않습니다"))
    }

    func testSuccessfulResourceSaveRestartsRunningVM() async {
        let backend = ResourceBackend(supportsChanges: true, setResult: true, running: true)
        let model = ControlModel(config: makeConfig(), backend: backend, startsAutomatically: false)
        model.pendingMemGiB = 4
        model.pendingCPU = 2

        model.applyResources()
        await waitUntilIdle(model, timeout: 4)

        XCTAssertEqual(backend.setCalls, 1)
        XCTAssertGreaterThanOrEqual(backend.livenessChecks, 1)
        XCTAssertEqual(backend.stopCalls, 1)
        XCTAssertEqual(backend.startCalls, 1)
        XCTAssertTrue(model.statusNote.contains("리소스 적용됨"))
    }

    func testFailedStopDoesNotLaunchSecondVMInstance() async {
        let backend = ResourceBackend(supportsChanges: true, setResult: true, running: true, stopsSuccessfully: false)
        let model = ControlModel(config: makeConfig(), backend: backend, startsAutomatically: false)

        model.applyResources()
        await waitUntilIdle(model, timeout: 4)

        XCTAssertEqual(backend.stopCalls, 1)
        XCTAssertEqual(backend.startCalls, 0)
        XCTAssertTrue(model.running)
        XCTAssertTrue(model.statusNote.contains("정지 실패"))
    }

    private func waitUntilIdle(_ model: ControlModel, timeout: TimeInterval = 1) async {
        let deadline = Date().addingTimeInterval(timeout)
        while model.busy && Date() < deadline {
            try? await Task.sleep(nanoseconds: 20_000_000)
        }
        XCTAssertFalse(model.busy)
    }

    private func makeConfig() -> VMConfig {
        VMConfig(id: "resource-test", name: "Resource Test", displayName: "Resource Test",
                 backendKind: "fast-vz", bootMode: nil, bundlePath: "", runnerPath: "",
                 launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                 leasesPath: "", guestName: "", displayWidth: 1, displayHeight: 1)
    }
}

private final class ResourceBackend: VMBackend {
    let displayName = "Resource Test"
    let kind = "test"
    let supportsGuestCommands = false
    let supportsPackageInstall = false
    let supportsClipboard = false
    let supportsSSH = false
    let supportsResourceChanges: Bool

    private let setResult: Bool
    private var running: Bool
    private let stopsSuccessfully: Bool
    private(set) var setCalls = 0
    private(set) var livenessChecks = 0
    private(set) var stopCalls = 0
    private(set) var startCalls = 0

    init(supportsChanges: Bool, setResult: Bool, running: Bool, stopsSuccessfully: Bool = true) {
        self.supportsResourceChanges = supportsChanges
        self.setResult = setResult
        self.running = running
        self.stopsSuccessfully = stopsSuccessfully
    }

    func isRunning() -> Bool { livenessChecks += 1; return running }
    func currentIP() -> String? { nil }
    func start() -> Bool { startCalls += 1; running = true; return true }
    func stop() { stopCalls += 1; if stopsSuccessfully { running = false } }
    func resources() -> (memMiB: Int, cpu: Int) { (4096, 2) }
    func setResources(memMiB: Int, cpu: Int) -> Bool { setCalls += 1; return setResult }
    func runInGuest(_ command: String) -> (output: String, code: Int32) { ("", -1) }
}
