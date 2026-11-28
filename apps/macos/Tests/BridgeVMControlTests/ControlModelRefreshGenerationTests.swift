import Combine
import XCTest
@testable import BridgeVMControl

@MainActor
final class ControlModelRefreshGenerationTests: XCTestCase {
    func testNewestRefreshWinsWhenQueriesCompleteOutOfOrder() async {
        let backend = DelayedRefreshBackend()
        let model = ControlModel(config: makeConfig(), backend: backend, startsAutomatically: false)
        model.running = true

        model.refreshStatus()
        XCTAssertEqual(backend.firstQueryEntered.wait(timeout: .now() + 1), .success)
        model.refreshStatus()
        await waitUntil { !model.running }

        backend.releaseFirstQuery.signal()
        try? await Task.sleep(nanoseconds: 100_000_000)

        XCTAssertFalse(model.running)
        XCTAssertEqual(model.statusNote, "VM exited")
    }

    /// Every 3 s poll that finds the VM as it was must not republish the model: the library rows and the
    /// dashboard observe it.
    func testAnUnchangedRunningStateDoesNotRepublishTheModel() {
        let model = ControlModel(config: makeConfig(), backend: DelayedRefreshBackend(), startsAutomatically: false)
        var changes = 0
        let observer = model.objectWillChange.sink { changes += 1 }
        defer { observer.cancel() }
        model.applyRuntimeStatus(running: false)
        XCTAssertEqual(changes, 0, "still stopped")
        model.applyRuntimeStatus(running: true)
        XCTAssertGreaterThan(changes, 0, "a start still shows at once")
        XCTAssertEqual(model.statusNote, "Running")
        let started = changes
        model.applyRuntimeStatus(running: true)
        XCTAssertEqual(changes, started, "still running")
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
    let firstQueryEntered = DispatchSemaphore(value: 0)
    let releaseFirstQuery = DispatchSemaphore(value: 0)

    private let lock = NSLock()
    private var queryCount = 0

    func isRunning() -> Bool {
        let call = lock.withLock { queryCount += 1; return queryCount }
        if call == 1 {
            firstQueryEntered.signal()
            _ = releaseFirstQuery.wait(timeout: .now() + 2)
            return true
        }
        return false
    }
    func resources() -> (memMiB: Int, cpu: Int) { (4096, 2) }
}
