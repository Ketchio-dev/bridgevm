import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

/// A fresh attempt that fails before finalization drops its large media, keeps
/// the evidence that explains the failure, and names what the user must repair.
@MainActor
final class HvfWindowsInstallStagingFailureTests: XCTestCase {
    typealias Fixture = HvfWindowsInstallStagingFixture

    func testFailedInstallerRunDiscardsStagedMediaAndKeepsItsLog() async throws {
        let f = try Fixture(diskGiB: 1)
        defer { f.remove() }
        let media = try f.media()
        let installer = FailingInstaller()

        let stage = try await runFreshAttempt(f.plan, installer)

        guard case let .failed(message) = stage else { return XCTFail("stage=\(stage)") }
        let log = media.evidence.appendingPathComponent("run.log")
        XCTAssertTrue(message.contains(log.path), message)
        XCTAssertEqual(installer.mediaPresentAtRun, [true], "the installer ran without prepared media")
        XCTAssertNil(Fixture.status(media.target), "the failed attempt kept its install target")
        XCTAssertNil(Fixture.status(media.vars), "the failed attempt kept its UEFI vars")
        XCTAssertEqual(try Data(contentsOf: log), FailingInstaller.log)
    }

    func testBundleOrMetadataOthersCanWriteIsRefusedWithThePermissionCause() async throws {
        for relative in ["", "metadata"] {
            let f = try Fixture(diskGiB: 1)
            defer { f.remove() }
            let directory = relative.isEmpty ? f.bundle : f.bundle.appendingPathComponent(relative)
            try FileManager.default.setAttributes([.posixPermissions: 0o775], ofItemAtPath: directory.path)
            let installer = FailingInstaller()

            let stage = try await runFreshAttempt(f.plan, installer)

            guard case let .failed(message) = stage else { return XCTFail("\(directory.path): stage=\(stage)") }
            XCTAssertEqual(installer.mediaPresentAtRun, [], directory.path)
            XCTAssertTrue(message.contains("현재 사용자만 쓸 수 있는 디렉터리가 아니어서 거부했습니다: \(directory.path)"),
                          message)
            XCTAssertFalse(message.contains("심볼릭 링크"), message)
        }
    }

    private func runFreshAttempt(
        _ plan: HvfWindowsInstallPlan, _ installer: FailingInstaller
    ) async throws -> HvfWindowsInstallStage {
        let queue = HvfWindowsInstallPipelineQueue()
        defer { queue.discard() }
        let session = HvfWindowsInstallSession(plan: plan, validate: { _ in nil }, schedule: queue.enqueue,
            execution: HvfWindowsInstallExecution(process: installer, verifySource: { _ in true }))
        let acknowledgment = try XCTUnwrap(session.start())
        await acknowledgment.value
        if !queue.jobs.isEmpty { await queue.runNext() }
        return session.stage
    }
}

/// Writes run.log where the scripted install does, then exits non-zero.
@MainActor
private final class FailingInstaller: HvfWindowsInstallProcessRunning {
    static let log = Data("BVINSTALL synthetic failure\n".utf8)
    private(set) var mediaPresentAtRun: [Bool] = []
    func reset() {}
    func cancel() {}
    func run(plan: HvfWindowsInstallPlan, arguments: [String], environment: [String: String],
             progressLog: URL?, output: @escaping (String) -> Void) async -> Bool {
        let value = { (flag: String) in arguments.firstIndex(of: flag).map { arguments[$0 + 1] } ?? "" }
        mediaPresentAtRun.append([value("--target"), value("--vars")].allSatisfy { FileManager.default.fileExists(atPath: $0) })
        try? Self.log.write(to: URL(fileURLWithPath: value("--evidence-dir")).appendingPathComponent("run.log"))
        return false
    }
}
