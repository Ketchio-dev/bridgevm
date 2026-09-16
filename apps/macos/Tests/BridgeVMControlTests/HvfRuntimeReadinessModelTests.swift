import Combine
import Foundation
import XCTest
@testable import BridgeVMControl

private final class RuntimeReadinessReaderGate: @unchecked Sendable {
    struct Snapshot {
        var requests: [(memory: Int, repo: URL)] = []
        var mainThreads: [Bool] = []
        var active = 0
        var maximumActive = 0
        var timedOut = false
    }
    private let condition = NSCondition()
    private let blocked: Set<Int>
    private var released: Set<Int> = []
    private var allReleased = false
    private var state = Snapshot()
    init(blocked: Set<Int> = [1]) { self.blocked = blocked }
    var snapshot: Snapshot { condition.lock(); defer { condition.unlock() }; return state }
    func release(_ call: Int) { condition.lock(); released.insert(call); condition.broadcast(); condition.unlock() }
    func releaseAll() { condition.lock(); allReleased = true; condition.broadcast(); condition.unlock() }
    func read(_ config: HvfEngineConfig, _ repo: URL) -> HvfWindowsReadinessReport {
        condition.lock()
        state.requests.append((config.ramMiB, repo)); state.mainThreads.append(Thread.isMainThread)
        let call = state.requests.count
        state.active += 1; state.maximumActive = max(state.maximumActive, state.active)
        let deadline = Date().addingTimeInterval(5)
        while blocked.contains(call), !allReleased, !released.contains(call) {
            if !condition.wait(until: deadline) { state.timedOut = true; break }
        }
        state.active -= 1
        condition.unlock()
        return .init(issues: [], productLimitations: ["memory=\(config.ramMiB);repo=\(repo.path);call=\(call)"])
    }
}

@MainActor
final class HvfRuntimeReadinessModelTests: XCTestCase {
    private let repo = URL(fileURLWithPath: "/unused/readiness-repo")
    private func config(_ memory: Int = 4096) -> HvfEngineConfig {
        .init(targetDiskPath: "/unused/disk", uefiVarsPath: "/unused/vars", evidenceDir: "/unused/evidence",
            watchdogMs: nil, ramMiB: memory, smpCpus: 2, clipboardSync: false, shareHostDir: nil,
            shareGuestDir: nil, virtioNet: false, audioEnabled: false, virtioGpu3d: false,
            nvmeBufferedIO: false, ctlFilePath: "/unused/control", swtpmBin: "/unused/swtpm")
    }
    private func wait(_ predicate: @MainActor () -> Bool) async throws {
        let deadline = ProcessInfo.processInfo.systemUptime + 2
        while !predicate(), ProcessInfo.processInfo.systemUptime < deadline {
            try await Task.sleep(nanoseconds: 5_000_000)
        }
        XCTAssertTrue(predicate(), "Bounded readiness fixture did not progress")
        if !predicate() { throw CocoaError(.executableRuntimeMismatch) }
    }

    func testBlockedReaderLeavesMainActorFreeAndUnknownReportFailsClosed() async throws {
        let gate = RuntimeReadinessReaderGate(); defer { gate.releaseAll() }
        let model = HvfRuntimeReadinessModel(reader: { gate.read($0, $1) }), config = config()
        XCTAssertNil(model.report(configuration: config, repoRoot: repo)); XCTAssertFalse(model.isChecking)
        model.request(configuration: config, repoRoot: repo)
        try await wait { gate.snapshot.active == 1 }
        XCTAssertTrue(Thread.isMainThread); XCTAssertTrue(model.isChecking)
        XCTAssertEqual(gate.snapshot.mainThreads, [false])
        XCTAssertNil(model.report(configuration: config, repoRoot: repo))
        gate.release(1); try await wait { !model.isChecking }
        XCTAssertNotNil(model.report(configuration: config, repoRoot: repo))
        XCTAssertFalse(gate.snapshot.timedOut)
    }

    func testManyChangesCoalesceToLatestConfigurationWithOneReader() async throws {
        let gate = RuntimeReadinessReaderGate(blocked: [1, 2]); defer { gate.releaseAll() }
        let model = HvfRuntimeReadinessModel(reader: { gate.read($0, $1) }), first = config()
        model.request(configuration: first, repoRoot: repo)
        try await wait { gate.snapshot.active == 1 }
        for memory in 4097...4196 { model.request(configuration: config(memory), repoRoot: repo) }
        XCTAssertEqual(gate.snapshot.requests.count, 1)
        gate.release(1); try await wait { gate.snapshot.requests.count == 2 }
        XCTAssertEqual(gate.snapshot.requests.map { $0.memory }, [4096, 4196])
        XCTAssertNil(model.report(configuration: first, repoRoot: repo))
        XCTAssertNil(model.report(configuration: config(4196), repoRoot: repo))
        gate.release(2); try await wait { !model.isChecking }
        XCTAssertNotNil(model.report(configuration: config(4196), repoRoot: repo))
        XCTAssertNil(model.report(configuration: first, repoRoot: repo))
        XCTAssertEqual(gate.snapshot.maximumActive, 1); XCTAssertFalse(gate.snapshot.timedOut)
    }

    func testDuplicateRequestsDoNotReadAgainAndForceInvalidatesCachedReport() async throws {
        let gate = RuntimeReadinessReaderGate(blocked: [2]); defer { gate.releaseAll() }
        let model = HvfRuntimeReadinessModel(reader: { gate.read($0, $1) }), config = config()
        model.request(configuration: config, repoRoot: repo)
        try await wait { !model.isChecking }
        let first = model.report(configuration: config, repoRoot: repo)
        for _ in 0..<20 { model.request(configuration: config, repoRoot: repo) }
        XCTAssertFalse(model.isChecking); XCTAssertEqual(gate.snapshot.requests.count, 1)
        model.request(configuration: config, repoRoot: repo, force: true)
        XCTAssertNil(model.report(configuration: config, repoRoot: repo))
        try await wait { gate.snapshot.requests.count == 2 }
        for _ in 0..<20 { model.request(configuration: config, repoRoot: repo) }
        gate.release(2); try await wait { !model.isChecking }
        XCTAssertEqual(gate.snapshot.requests.count, 2)
        XCTAssertNotEqual(model.report(configuration: config, repoRoot: repo), first)
        XCTAssertEqual(gate.snapshot.maximumActive, 1)
    }

    func testForceWhileReadingAndRepositoryChangeDiscardOldCompletion() async throws {
        let gate = RuntimeReadinessReaderGate(blocked: [1, 2]); defer { gate.releaseAll() }
        let model = HvfRuntimeReadinessModel(reader: { gate.read($0, $1) }), config = config()
        let other = URL(fileURLWithPath: "/unused/other-repo")
        model.request(configuration: config, repoRoot: repo)
        try await wait { gate.snapshot.active == 1 }
        for _ in 0..<20 { model.request(configuration: config, repoRoot: repo, force: true) }
        model.request(configuration: config, repoRoot: other)
        gate.release(1); try await wait { gate.snapshot.requests.count == 2 }
        XCTAssertEqual(gate.snapshot.requests.map { $0.repo }, [repo, other])
        XCTAssertNil(model.report(configuration: config, repoRoot: repo))
        XCTAssertNil(model.report(configuration: config, repoRoot: other))
        gate.release(2); try await wait { !model.isChecking }
        XCTAssertNotNil(model.report(configuration: config, repoRoot: other))
        XCTAssertNil(model.report(configuration: config, repoRoot: repo))
        XCTAssertEqual(gate.snapshot.requests.count, 2); XCTAssertEqual(gate.snapshot.maximumActive, 1)
    }

    func testNotificationReentryReservesNextReadWithoutDuplicateWorkerOrStaleReport() async throws {
        let gate = RuntimeReadinessReaderGate(blocked: []); defer { gate.releaseAll() }
        let model = HvfRuntimeReadinessModel(reader: { gate.read($0, $1) }), config = config()
        var refreshed = false
        let observer = model.objectWillChange.sink {
            if !model.isChecking, !refreshed {
                refreshed = true
                model.request(configuration: config, repoRoot: self.repo, force: true)
                XCTAssertTrue(model.isChecking)
                XCTAssertNil(model.report(configuration: config, repoRoot: self.repo))
            } else { model.request(configuration: config, repoRoot: self.repo) }
        }
        defer { observer.cancel() }
        model.request(configuration: config, repoRoot: repo)
        try await wait { refreshed && !model.isChecking }
        XCTAssertEqual(gate.snapshot.requests.count, 2); XCTAssertEqual(gate.snapshot.maximumActive, 1)
        XCTAssertTrue(model.report(configuration: config, repoRoot: repo)?.productLimitations.first?.hasSuffix("call=2") == true)
    }
}
