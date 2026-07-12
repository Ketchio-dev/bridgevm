import XCTest
@testable import BridgeVMControl

@MainActor
final class ControlModelRefreshGenerationTests: XCTestCase {
    func testLifecycleActionInvalidatesOlderStatusQuery() async {
        let backend = DelayedRefreshBackend(startResult: false)
        let model = ControlModel(config: makeConfig(), backend: backend, startsAutomatically: false)

        model.refreshStatus()
        XCTAssertEqual(backend.firstQueryEntered.wait(timeout: .now() + 1), .success)
        model.start()
        await waitUntil { !model.lifecycleBusy }
        XCTAssertEqual(model.statusNote, "VM 시작 실패")

        backend.releaseFirstQuery.signal()
        try? await Task.sleep(nanoseconds: 100_000_000)

        XCTAssertFalse(model.running)
        XCTAssertEqual(model.ip, "—")
        XCTAssertEqual(model.statusNote, "VM 시작 실패")
    }

    func testNewestRefreshWinsWhenQueriesCompleteOutOfOrder() async {
        let backend = DelayedRefreshBackend(startResult: true)
        let model = ControlModel(config: makeConfig(), backend: backend, startsAutomatically: false)
        model.running = true
        model.ip = "old"

        model.refreshStatus()
        XCTAssertEqual(backend.firstQueryEntered.wait(timeout: .now() + 1), .success)
        model.refreshStatus()
        await waitUntil { model.ip == "—" }

        backend.releaseFirstQuery.signal()
        try? await Task.sleep(nanoseconds: 100_000_000)

        XCTAssertFalse(model.running)
        XCTAssertEqual(model.ip, "—")
    }

    private func waitUntil(timeout: TimeInterval = 1, _ condition: @escaping () -> Bool) async {
        let deadline = Date().addingTimeInterval(timeout)
        while !condition() && Date() < deadline {
            try? await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTAssertTrue(condition())
    }

    private func makeConfig() -> VMConfig {
        VMConfig(id: "refresh-test", name: "Refresh Test", displayName: "Refresh Test",
                 backendKind: "fast-vz", bootMode: nil, bundlePath: "", runnerPath: "",
                 launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                 leasesPath: "", guestName: "", displayWidth: 1, displayHeight: 1)
    }
}

private final class DelayedRefreshBackend: VMBackend {
    let displayName = "Refresh Test"
    let kind = "test"
    let supportsGuestCommands = false
    let supportsPackageInstall = false
    let supportsClipboard = false
    let supportsSSH = false
    let supportsResourceChanges = false
    let firstQueryEntered = DispatchSemaphore(value: 0)
    let releaseFirstQuery = DispatchSemaphore(value: 0)

    private let lock = NSLock()
    private let startResult: Bool
    private var queryCount = 0

    init(startResult: Bool) { self.startResult = startResult }

    func isRunning() -> Bool {
        let call = lock.withLock { queryCount += 1; return queryCount }
        if call == 1 {
            firstQueryEntered.signal()
            _ = releaseFirstQuery.wait(timeout: .now() + 2)
            return true
        }
        return false
    }
    func currentIP() -> String? { "stale" }
    func start() -> Bool { startResult }
    func stop() {}
    func resources() -> (memMiB: Int, cpu: Int) { (4096, 2) }
    func setResources(memMiB: Int, cpu: Int) -> Bool { false }
    func runInGuest(_ command: String) -> (output: String, code: Int32) { ("", -1) }
}
