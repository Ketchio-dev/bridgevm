import Foundation
import XCTest
@testable import BridgeVMControl

private final class InstallFinalizationProbe: @unchecked Sendable {
    enum Failure: Error, Equatable { case expected }
    private let lock = NSLock()
    private let entered = DispatchSemaphore(value: 0)
    private let released = DispatchSemaphore(value: 0)
    private var mainThreads: [Bool] = []
    private var failure: Failure?

    init(failure: Failure? = nil) { self.failure = failure }

    func finalize(_ plan: HvfWindowsInstallPlan) throws {
        lock.lock(); mainThreads.append(Thread.isMainThread); let failure = failure; lock.unlock()
        entered.signal()
        if !Thread.isMainThread { _ = released.wait(timeout: .now() + 5) }
        if let failure { throw failure }
    }

    func waitForEntry() async -> Bool {
        await Task.detached { [entered] in waitForSemaphore(entered) }.value
    }

    func release() { released.signal() }

    var observedMainThreads: [Bool] {
        lock.lock(); defer { lock.unlock() }; return mainThreads
    }
}

private func waitForSemaphore(_ semaphore: DispatchSemaphore) -> Bool {
    semaphore.wait(timeout: .now() + 5) == .success
}

@MainActor
final class HvfWindowsInstallFinalizationWorkerTests: XCTestCase {
    func testFinalizationRunsOffMainThreadAndReturnsToMainActor() async throws {
        let probe = InstallFinalizationProbe()
        let worker = HvfWindowsInstallFinalizationWorker(finalize: { try probe.finalize($0) })
        let task = Task { try await worker.run(HvfWindowsInstallTestSupport.plan()) }
        let entered = await probe.waitForEntry(); XCTAssertTrue(entered)
        XCTAssertTrue(Thread.isMainThread)
        XCTAssertEqual(probe.observedMainThreads, [false])
        probe.release()
        try await task.value
        XCTAssertTrue(Thread.isMainThread)
        XCTAssertEqual(probe.observedMainThreads.count, 1)
    }

    func testFinalizationErrorPropagatesUnchanged() async {
        let probe = InstallFinalizationProbe(failure: .expected)
        let worker = HvfWindowsInstallFinalizationWorker(finalize: { try probe.finalize($0) })
        let task = Task { try await worker.run(HvfWindowsInstallTestSupport.plan()) }
        let entered = await probe.waitForEntry(); XCTAssertTrue(entered)
        probe.release()
        do { try await task.value; XCTFail("Expected finalization failure") }
        catch { XCTAssertEqual(error as? InstallFinalizationProbe.Failure, .expected) }
        XCTAssertEqual(probe.observedMainThreads, [false])
    }

    func testCancellingAwaiterDoesNotAbandonAdmittedFinalization() async throws {
        let probe = InstallFinalizationProbe()
        let worker = HvfWindowsInstallFinalizationWorker(finalize: { try probe.finalize($0) })
        let task = Task { try await worker.run(HvfWindowsInstallTestSupport.plan()) }
        let entered = await probe.waitForEntry(); XCTAssertTrue(entered)
        task.cancel()
        probe.release()
        try await task.value
        XCTAssertEqual(probe.observedMainThreads, [false])
    }
}
