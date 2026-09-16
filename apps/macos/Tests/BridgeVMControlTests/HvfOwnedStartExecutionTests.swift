import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfOwnedStartTestClock: @unchecked Sendable {
    private let lock = NSLock()
    private var stored: TimeInterval = 100
    var value: TimeInterval {
        get { lock.lock(); defer { lock.unlock() }; return stored }
        set { lock.lock(); stored = newValue; lock.unlock() }
    }
}
actor HvfOwnedStartWorkerGate {
    private var continuation: CheckedContinuation<Void, Never>?
    func run(_ input: HvfOwnedStartExecution.Input) async -> Result<HvfOwnedRuntimeSpawn, HvfOwnedStartFailure> {
        await withCheckedContinuation { continuation = $0 }
        guard await input.permit() else { return .failure(.configurationChanged) }
        return .failure(.readiness)
    }
    var waiting: Bool { continuation != nil }
    func release() { continuation?.resume(); continuation = nil }
}

@MainActor
final class HvfOwnedStartExecutionTests: XCTestCase {
    func testDeadlineKeepsUnfinishedWorkerOwnedAndPreventsLateEffects() async throws {
        let f = try HvfOwnedRuntimeFixture(); defer { f.clean() }
        let session = f.session(), clock = HvfOwnedStartTestClock(), gate = HvfOwnedStartWorkerGate()
        session.ownedStartNow = { clock.value }; session.ownedStartWorker = { await gate.run($0) }
        let ticket = try HvfOwnedStartTestSupport.admit(session)
        let deadline = ProcessInfo.processInfo.systemUptime + 2
        while !(await gate.waiting), ProcessInfo.processInfo.systemUptime < deadline { try await Task.sleep(nanoseconds: 5_000_000) }
        let waiting = await gate.waiting; XCTAssertTrue(waiting)
        clock.value = 131
        XCTAssertEqual(ticket.observation.phase, .unconfirmed, "Direct retained ticket reads enforce the original deadline")
        XCTAssertEqual(ticket.observation.failure, .deadlineExceeded)
        XCTAssertTrue(session.hasPendingOwnedStart)
        XCTAssertNil(session.reserveRuntimeMutation(kind: .snapshot))
        if case .refused = session.start() {} else { XCTFail("Unfinished worker must block UI start") }
        await gate.release()
        try await HvfOwnedStartTestSupport.wait { !ticket.workerPending }
        XCTAssertEqual(ticket.observation.phase, .failed)
        XCTAssertEqual(ticket.observation.failure, .deadlineExceeded)
        XCTAssertFalse(session.hasActiveRuntimeWork); XCTAssertEqual(f.launches, 0)
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.config.evidenceDir))
    }

    func testConfigurationChangeInvalidatesDetachedEffectPermission() async throws {
        let f = try HvfOwnedRuntimeFixture(); defer { f.clean() }
        let session = f.session(), gate = HvfOwnedStartWorkerGate()
        session.ownedStartWorker = { await gate.run($0) }
        let ticket = try HvfOwnedStartTestSupport.admit(session)
        let deadline = ProcessInfo.processInfo.systemUptime + 2
        while !(await gate.waiting), ProcessInfo.processInfo.systemUptime < deadline { try await Task.sleep(nanoseconds: 5_000_000) }
        session.config.ramMiB += 128
        XCTAssertFalse(ticket.effectAdmission.isValid)
        XCTAssertEqual(ticket.observation.failure, .configurationChanged)
        XCTAssertTrue(ticket.workerPending)
        await gate.release(); try await HvfOwnedStartTestSupport.wait { !ticket.workerPending }
        XCTAssertEqual(ticket.observation.phase, .failed); XCTAssertEqual(f.launches, 0)
    }

    func testProductionWorkerRefusesLegacyFallbackBeforePreparation() async throws {
        let f = try HvfOwnedRuntimeFixture(); defer { f.clean() }
        let input = HvfOwnedStartExecution.Input(config: f.config, repoRoot: f.root, keyProvider: nil,
            launch: { _ in XCTFail("Legacy wrapper cannot substitute for typed runner") }, deadline: 130,
            now: { 100 }, effectAdmission: HvfRuntimeEffectAdmission(), permit: { true })
        let result = await HvfOwnedStartExecution.run(input)
        guard case .failure(.helperUnavailable) = result else { return XCTFail("Typed runner required") }
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.config.evidenceDir))
    }

    func testProductionWorkerRechecksDeadlineBeforePreparation() async throws {
        let f = try HvfOwnedRuntimeFixture(); defer { f.clean() }
        let runner = f.root.appendingPathComponent("target/release/hvf-runner")
        try Data("never executed".utf8).write(to: runner)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: runner.path)
        let clock = HvfOwnedStartTestClock(); var permits = 0
        let input = HvfOwnedStartExecution.Input(config: f.config, repoRoot: f.root, keyProvider: nil,
            launch: { _ in XCTFail("Expired preparation cannot spawn") }, deadline: 130,
            now: { clock.value }, effectAdmission: HvfRuntimeEffectAdmission(), permit: {
                permits += 1; if permits == 2 { clock.value = 131 }; return true
            })
        let result = await HvfOwnedStartExecution.run(input)
        guard case .failure(.deadlineExceeded) = result else { return XCTFail("Original deadline must apply before preparation") }
        XCTAssertEqual(permits, 2)
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.config.evidenceDir))
    }
    func testExpiredOrRemovedAdmissionBeforeExistingKeyLookupHasNoKeyEffects() async throws {
        final class Keys: VTPMExistingStateKeyReading {
            var reads = 0
            func existingStateKey(for stableVMID: String) throws -> Data {
                reads += 1; return Data(repeating: 0x42, count: 32)
            }
        }
        for expired in [false, true] {
            let f = try HvfOwnedRuntimeFixture(encrypted: true); defer { f.clean() }
            let runner = f.root.appendingPathComponent("target/release/hvf-runner")
            try Data("never executed".utf8).write(to: runner)
            try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: runner.path)
            let keys = Keys(), clock = HvfOwnedStartTestClock(); var permits = 0
            let input = HvfOwnedStartExecution.Input(config: f.config, repoRoot: f.root, keyProvider: keys,
                launch: { _ in XCTFail("Refused key lookup cannot spawn") }, deadline: 130,
                now: { clock.value }, effectAdmission: HvfRuntimeEffectAdmission(), permit: {
                    permits += 1
                    if permits == 2 { if expired { clock.value = 131 }; return expired }
                    return true
                })
            let result = await HvfOwnedStartExecution.run(input)
            guard case let .failure(reason) = result else { return XCTFail("Late key admission must fail") }
            XCTAssertEqual(reason, expired ? .deadlineExceeded : .configurationChanged)
            XCTAssertEqual(permits, 2); XCTAssertEqual(keys.reads, 0)
            XCTAssertFalse(FileManager.default.fileExists(atPath: f.config.evidenceDir))
        }
    }

}
