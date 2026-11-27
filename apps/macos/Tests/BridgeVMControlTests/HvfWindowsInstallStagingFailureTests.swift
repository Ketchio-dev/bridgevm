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

        let result = try await runFreshAttempt(f.plan, installer)

        guard case let .failed(message) = result.stage else { return XCTFail("stage=\(result.stage)") }
        let log = media.evidence.appendingPathComponent("run.log")
        XCTAssertTrue(message.contains(log.path), message)
        XCTAssertEqual(installer.mediaPresentAtRun, [true], "the installer ran without prepared media")
        XCTAssertNil(Fixture.status(media.target), "the failed attempt kept its install target")
        XCTAssertNil(Fixture.status(media.vars), "the failed attempt kept its UEFI vars")
        XCTAssertEqual(try Data(contentsOf: log), FailingInstaller.log)
    }

    /// A full Mac disk paused the install VM, which then sat on "Installing Windows" until the 25-minute
    /// watchdog and failed with the general sentence (the freed target hid the cause).
    func testAFullMacDiskStopsTheInstallAndSaysSo() async throws {
        let f = try Fixture(diskGiB: 1)
        defer { f.remove() }
        XCTAssertTrue(HvfWindowsInstallSession.isProgressLine("BVSTORAGE full errno=28 free=0 path=/"))
        let installer = FailingInstaller(output: "BVSTORAGE full errno=28 free=0 path=/")

        let result = try await runFreshAttempt(f.plan, installer)

        XCTAssertEqual(result.stage, .failed(HvfWindowsInstallSession.diskFullMessage))
        XCTAssertEqual(installer.cancels, 1, "the paused VM is stopped at once")
        let builder = FailingInstaller(output: "rsync: write failed on \"sources/install.wim\": No space left on device (28)")
        let build = try await runFreshAttempt(f.plan, builder, sourceVerified: false)
        XCTAssertEqual(build.stage, .failed(HvfWindowsInstallSession.diskFullMessage))
    }

    func testBundleOrMetadataOthersCanWriteIsRefusedWithThePermissionCause() async throws {
        for relative in ["", "metadata"] {
            let f = try Fixture(diskGiB: 1)
            defer { f.remove() }
            let directory = relative.isEmpty ? f.bundle : f.bundle.appendingPathComponent(relative)
            try FileManager.default.setAttributes([.posixPermissions: 0o775], ofItemAtPath: directory.path)
            let installer = FailingInstaller()

            let result = try await runFreshAttempt(f.plan, installer)

            guard case let .failed(message) = result.stage else { return XCTFail("\(directory.path): stage=\(result.stage)") }
            XCTAssertEqual(installer.mediaPresentAtRun, [], directory.path)
            XCTAssertEqual(message, WindowsFileFailureFeedback.preparation)
            XCTAssertTrue(result.log.contains { $0.contains("The installation folder must be owned by the current user and writable only by that user: \(directory.path)") }, "permission cause must remain in diagnostics")
            XCTAssertFalse(message.contains("symbolic link"), message)
        }
    }

    func testAnX64ISOIsNamedInsteadOfTheGeneralSetupFilesSentence() async throws {
        let f = try Fixture(diskGiB: 1)
        defer { f.remove() }
        let builder = FailingInstaller(output: "FAIL: not-arm64-iso: the ISO has no efi/boot/bootaa64.efi, so it is not Windows for Arm")

        let result = try await runFreshAttempt(f.plan, builder, sourceVerified: false)

        XCTAssertEqual(result.stage, .failed(HvfWindowsInstallSession.notArm64ISO))
    }

    private func runFreshAttempt(
        _ plan: HvfWindowsInstallPlan, _ installer: FailingInstaller, sourceVerified: Bool = true
    ) async throws -> (stage: HvfWindowsInstallStage, log: [String]) {
        let queue = HvfWindowsInstallPipelineQueue()
        defer { queue.discard() }
        let session = HvfWindowsInstallSession(plan: plan, validate: { _ in nil }, schedule: queue.enqueue,
            execution: HvfWindowsInstallExecution(process: installer, verifySource: { _ in sourceVerified }))
        let acknowledgment = try XCTUnwrap(session.start())
        await acknowledgment.value
        if !queue.jobs.isEmpty { await queue.runNext() }
        return (session.stage, session.logLines)
    }
}

/// Writes run.log where the scripted install does, then exits non-zero.
@MainActor
private final class FailingInstaller: HvfWindowsInstallProcessRunning {
    static let log = Data("BVINSTALL synthetic failure\n".utf8)
    private(set) var mediaPresentAtRun: [Bool] = []
    /// A line the failing command prints first, as the source builder does.
    let outputLine: String?
    init(output: String? = nil) { outputLine = output }
    private(set) var cancels = 0
    func reset() {}
    func cancel() { cancels += 1 }
    func run(plan: HvfWindowsInstallPlan, arguments: [String], environment: [String: String],
             progressLog: URL?, output: @escaping (String) -> Void) async -> Bool {
        if let outputLine { output(outputLine); return false }
        let value = { (flag: String) in arguments.firstIndex(of: flag).map { arguments[$0 + 1] } ?? "" }
        mediaPresentAtRun.append([value("--target"), value("--vars")].allSatisfy { FileManager.default.fileExists(atPath: $0) })
        try? Self.log.write(to: URL(fileURLWithPath: value("--evidence-dir")).appendingPathComponent("run.log"))
        return false
    }
}
