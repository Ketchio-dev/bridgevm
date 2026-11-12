import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallPreparationResponsiveTests: XCTestCase {
    private final class Gate: @unchecked Sendable {
        let entered = DispatchSemaphore(value: 0)
        let release = DispatchSemaphore(value: 0)
        let completed = DispatchSemaphore(value: 0)
        func prepare(_ plan: HvfWindowsInstallPlan) throws {
            entered.signal()
            _ = release.wait(timeout: .now() + 2)
            defer { completed.signal() }
            try FileManager.default.createDirectory(atPath: plan.stagingDirectory, withIntermediateDirectories: true)
            try Data("prepared".utf8).write(to: URL(fileURLWithPath: plan.stagingVarsPath))
        }
    }

    func testAdmissionRevokedDuringPreparationStopsBeforeInstall() async throws {
        let plan = HvfWindowsInstallTestSupport.plan()
        defer { try? FileManager.default.removeItem(at: plan.repoRoot) }
        try FileManager.default.createDirectory(atPath: plan.bundlePath + "/metadata", withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: URL(fileURLWithPath: plan.sourceImagePath).deletingLastPathComponent(), withIntermediateDirectories: true)
        let process = HvfWindowsInstallCancellationProcess(gate: HvfWindowsInstallCancellationGate(), holdProcess: false)
        let execution = HvfWindowsInstallExecution(process: process, verifySource: { _ in true },
            prepareMedia: { plan in
                try FileManager.default.createDirectory(atPath: plan.stagingDirectory, withIntermediateDirectories: true)
                try Data("prepared".utf8).write(to: URL(fileURLWithPath: plan.stagingVarsPath))
            }, finalize: { _ in XCTFail("refused preparation must not finalize") })
        let session = HvfWindowsInstallSession(plan: plan, execution: execution)
        session.attemptActive = true
        session.continuationAdmission = { "VM 등록 정보가 변경되었습니다." }
        await session.run()
        XCTAssertEqual(session.stage, .failed("VM 등록 정보가 변경되었습니다."))
        XCTAssertEqual(process.requests, 0)
        XCTAssertFalse(FileManager.default.fileExists(atPath: plan.stagingVarsPath))
    }

    func testPreparationYieldsAndCancellationCleansBeforeInstall() async throws {
        let plan = HvfWindowsInstallTestSupport.plan()
        defer { try? FileManager.default.removeItem(at: plan.repoRoot) }
        try FileManager.default.createDirectory(atPath: plan.bundlePath + "/metadata", withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: URL(fileURLWithPath: plan.sourceImagePath).deletingLastPathComponent(), withIntermediateDirectories: true)
        let gate = Gate()
        let processGate = HvfWindowsInstallCancellationGate()
        let process = HvfWindowsInstallCancellationProcess(gate: processGate, holdProcess: false)
        let execution = HvfWindowsInstallExecution(process: process, verifySource: { _ in true },
            preparationWorker: gate.prepare, finalize: { _ in XCTFail("cancelled preparation must not finalize") })
        let session = HvfWindowsInstallSession(plan: plan, execution: execution)
        session.attemptActive = true
        let work = Task { await session.run() }
        let entered = await Task.detached { gate.entered.wait(timeout: .now() + 3) == .success }.value
        XCTAssertTrue(entered)
        XCTAssertEqual(gate.completed.wait(timeout: .now()), .timedOut, "preparation must yield the main actor")
        session.requestTerminationStop()
        XCTAssertTrue(session.cancelled)
        XCTAssertFalse(session.requiresTerminationWait, "preparation remains pre-commit")
        XCTAssertTrue(session.blocksAppTermination, "cleanup retains the reservation")
        gate.release.signal()
        await work.value
        XCTAssertEqual(session.stage, .cancelled)
        XCTAssertEqual(process.requests, 0, "no next-stage process may start")
        XCTAssertFalse(session.blocksAppTermination)
        XCTAssertFalse(FileManager.default.fileExists(atPath: plan.stagingTargetPath))
        XCTAssertFalse(FileManager.default.fileExists(atPath: plan.stagingVarsPath))
        // The existing cleanup contract retains the staging directory and diagnostic evidence.
    }
}
