import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallCancellationGate {
    private var continuation: CheckedContinuation<Void, Never>?
    private var timeout: Task<Void, Never>?
    private(set) var entered = false
    private(set) var timedOut = false

    func hold() async {
        entered = true
        await withCheckedContinuation { continuation in
            self.continuation = continuation
            timeout = Task { [weak self] in
                do { try await Task.sleep(nanoseconds: 5_000_000_000) } catch { return }
                guard let self, self.continuation != nil else { return }
                self.timedOut = true
                self.release()
            }
        }
    }

    func waitForEntry() async -> Bool {
        let deadline = Date().addingTimeInterval(5)
        while !entered && Date() < deadline { try? await Task.sleep(nanoseconds: 10_000_000) }
        return entered
    }

    func release() {
        timeout?.cancel()
        timeout = nil
        let pending = continuation
        continuation = nil
        pending?.resume()
    }
}

@MainActor
final class HvfWindowsInstallCancellationProcess: HvfWindowsInstallProcessRunning {
    let gate: HvfWindowsInstallCancellationGate
    let holdProcess: Bool
    var requests = 0
    var cancellations = 0
    var resets = 0
    private var cancelled = false

    init(gate: HvfWindowsInstallCancellationGate, holdProcess: Bool) {
        self.gate = gate
        self.holdProcess = holdProcess
    }
    func reset() { resets += 1; cancelled = false }
    func cancel() { cancellations += 1; cancelled = true }
    func run(plan: HvfWindowsInstallPlan, arguments: [String], environment: [String: String],
        progressLog: URL?, output: @escaping (String) -> Void) async -> Bool {
        requests += 1
        guard !cancelled else { return false }
        if holdProcess && requests == 1 { await gate.hold() }
        // Model a child that exited successfully just before cancellation was
        // requested, while its awaiting session has not yet resumed.
        return true
    }
}

@MainActor
final class HvfWindowsInstallCancellationFixture {
    enum Boundary { case cache, source, install }
    let plan = HvfWindowsInstallTestSupport.plan()
    let gate = HvfWindowsInstallCancellationGate()
    let queue = HvfWindowsInstallPipelineQueue()
    let boundary: Boundary
    let cacheVerified: Bool
    let sentinel = Data("owned-before-cancellation".utf8)
    lazy var process = HvfWindowsInstallCancellationProcess(gate: gate, holdProcess: boundary != .cache)
    private(set) var prepares = 0
    private(set) var finalizations = 0
    private(set) var completions = 0
    lazy var session: HvfWindowsInstallSession = {
        let execution = HvfWindowsInstallExecution(process: process,
            verifySource: { [weak self] _ in
                guard let self else { return false }
                if self.boundary == .cache { await self.gate.hold() }
                return self.cacheVerified
            }, prepareMedia: { [weak self] plan in
                guard let self else { throw CocoaError(.fileWriteUnknown) }
                self.prepares += 1
                try Data("owned-prepared-vars".utf8).write(to: URL(fileURLWithPath: plan.tmpVarsPath))
            }, finalize: { [weak self] _ in self?.finalizations += 1 })
        let session = HvfWindowsInstallSession(plan: plan, validate: { _ in nil },
            schedule: queue.enqueue, execution: execution)
        session.onCompleted = { [weak self] in self?.completions += 1 }
        return session
    }()

    init(boundary: Boundary, cacheVerified: Bool) throws {
        self.boundary = boundary
        self.cacheVerified = cacheVerified
        let source = URL(fileURLWithPath: plan.sourceImagePath)
        try FileManager.default.createDirectory(at: source.deletingLastPathComponent(), withIntermediateDirectories: true)
        for path in [plan.sourceImagePath, plan.tmpTargetPath, plan.tmpVarsPath] {
            try sentinel.write(to: URL(fileURLWithPath: path), options: .withoutOverwriting)
        }
    }

    func whilePaused(_ body: (HvfWindowsInstallSession) throws -> Void) async throws {
        let acknowledgment = try XCTUnwrap(session.start())
        await acknowledgment.value
        let work = Task { await queue.runNext() }
        let entered = await gate.waitForEntry()
        XCTAssertTrue(entered, "The actual pipeline must reach its bounded injected await")
        do { if entered { try body(session) } }
        catch { gate.release(); await work.value; throw error }
        gate.release()
        await work.value
        XCTAssertFalse(gate.timedOut)
    }

    func cancelAndAssertReserved(_ session: HvfWindowsInstallSession) {
        session.cancel()
        session.cancel()
        XCTAssertEqual(session.stage, .cancelling)
        XCTAssertTrue(session.isRunning)
        XCTAssertNil(session.start(), "An unacknowledged cancellation must retain admission")
        XCTAssertEqual(process.cancellations, 1)
        XCTAssertEqual(session.logLines.filter { $0 == "사용자가 설치를 취소했습니다." }.count, 1)
        XCTAssertTrue(queue.jobs.isEmpty)
    }

    func assertCancelled() {
        XCTAssertEqual(session.stage, .cancelled)
        XCTAssertFalse(session.isRunning)
        XCTAssertEqual(finalizations, 0)
        XCTAssertEqual(completions, 0)
        let lines = session.logLines
        session.cancel()
        XCTAssertEqual(session.logLines, lines)
        XCTAssertEqual(process.cancellations, 1)
    }

    func clean() {
        gate.release()
        queue.discard()
        try? FileManager.default.removeItem(at: plan.repoRoot)
        for path in [plan.tmpTargetPath, plan.tmpVarsPath, plan.tmpEvidenceDir] {
            try? FileManager.default.removeItem(atPath: path)
        }
    }
}
