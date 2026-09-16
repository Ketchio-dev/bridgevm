import CryptoKit
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallRecoveryProcess: HvfWindowsInstallProcessRunning {
    private(set) var requests = 0, resets = 0, cancellations = 0
    let originalDisk: Data
    init(originalDisk: Data) { self.originalDisk = originalDisk }
    func reset() { resets += 1 }
    func cancel() { cancellations += 1 }
    func run(plan: HvfWindowsInstallPlan, arguments: [String], environment: [String: String],
             progressLog: URL?, output: @escaping (String) -> Void) async -> Bool {
        requests += 1
        // A second installer run visibly replaces, rather than accidentally matching, the sealed source.
        let bytes = requests == 1 ? originalDisk : Data(repeating: 0x52, count: originalDisk.count)
        do { try bytes.write(to: URL(fileURLWithPath: plan.tmpTargetPath)); return true }
        catch { XCTFail("Synthetic installer write failed: \(error)"); return false }
    }
}

@MainActor
final class HvfWindowsInstallRecoveryFixture {
    struct InjectedFinalizationFailure: Error {}
    let root: URL
    let plan: HvfWindowsInstallPlan
    let config: VMConfig
    let boundary: HvfWindowsInstallFinalizationBoundary
    let originalDisk = Data(repeating: 0x41, count: 4096)
    let originalVars = Data(repeating: 0x31, count: 8192)
    let validator: HvfWindowsInstallValidationProbe
    let recoveryProbe = HvfWindowsInstallRecoveryProbe()
    let queue = HvfWindowsInstallPipelineQueue()
    lazy var process = HvfWindowsInstallRecoveryProcess(originalDisk: originalDisk)
    private(set) var cacheChecks = 0, prepares = 0, finalizations = 0, completions = 0
    private var injectedFailure = false
    lazy var session: HvfWindowsInstallSession = {
        let execution = HvfWindowsInstallExecution(process: process,
            verifySource: { [weak self] _ in self?.cacheChecks += 1; return self != nil },
            prepareMedia: { [weak self] plan in
                guard let self else { throw CocoaError(.fileWriteUnknown) }
                self.prepares += 1
                let bytes = self.prepares == 1 ? self.originalVars : Data(repeating: 0x62, count: self.originalVars.count)
                try bytes.write(to: URL(fileURLWithPath: plan.tmpVarsPath))
            }, finalize: { [weak self] plan in
                guard let self else { throw CocoaError(.fileWriteUnknown) }
                self.finalizations += 1
                try self.interruptFinalization(plan)
            })
        let probe = validator, recovery = recoveryProbe
        let value = HvfWindowsInstallSession(plan: plan, validate: { probe.validate($0) },
            schedule: queue.enqueue, execution: execution,
            recovery: .init(inspect: { try recovery.inspect($0) }, recover: { try recovery.recover($0, ticket: $1) }))
        value.onCompleted = { [weak self] in self?.completions += 1 }
        return value
    }()

    init(boundary: HvfWindowsInstallFinalizationBoundary, gateValidation: Bool = false) throws {
        self.boundary = boundary
        validator = HvfWindowsInstallValidationProbe(gateFirstCall: gateValidation)
        root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("install-recovery-" + UUID().uuidString)
        let slug = "recovery-" + UUID().uuidString.lowercased()
        let library = root.appendingPathComponent("library"), bundle = root.appendingPathComponent("bundle")
        config = VMConfig(id: slug, name: slug, displayName: "Synthetic recovery", backendKind: "hvf-engine",
            bootMode: "iso-efi", bundlePath: bundle.path, runnerPath: "/unused", launchSpecPath: "/unused",
            handoffPath: "/unused", sshKeyPath: "/unused", sshUser: "fixture", leasesPath: "/unused",
            guestName: "fixture", displayWidth: 1280, displayHeight: 720, installPending: true,
            memMiB: 4096, cpuCount: 4, networkEnabled: false, experimental3DAllowed: false)
        let request = HvfWindowsInstallRequest(isoPath: root.appendingPathComponent("synthetic.iso").path,
            diskGiB: 64, injectViogpu3d: false, driverPackageDir: nil)
        plan = HvfWindowsInstallPlan(repoRoot: root, libraryRoot: library,
            bundlePath: bundle.path, slug: slug, request: request)
        try FileManager.default.createDirectory(at: bundle, withIntermediateDirectories: true)
        XCTAssertTrue(VMLibrary.save(config, rootURL: library)); XCTAssertTrue(request.save(bundlePath: bundle.path))
        try FileManager.default.createDirectory(at: URL(fileURLWithPath: plan.sourceImagePath).deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("synthetic verified source marker".utf8).write(to: URL(fileURLWithPath: plan.sourceImagePath))
    }
    var paths: HvfWindowsInstallFinalizationPaths {
        .init(libraryRoot: plan.libraryRoot, bundle: URL(fileURLWithPath: plan.bundlePath), slug: plan.slug)
    }
    func interruptFinalization(_ plan: HvfWindowsInstallPlan) throws {
        try HvfWindowsInstallFinalization.finalize(plan: plan, faultInjector: { [self] reached in
            if !injectedFailure, reached == boundary {
                injectedFailure = true; throw InjectedFinalizationFailure()
            }
        }, secureBootSeeder: Self.syntheticSeeder)
    }
    func createInterruptedTransaction() throws {
        try originalDisk.write(to: URL(fileURLWithPath: plan.tmpTargetPath))
        try originalVars.write(to: URL(fileURLWithPath: plan.tmpVarsPath))
        XCTAssertThrowsError(try interruptFinalization(plan))
        XCTAssertTrue(FileManager.default.fileExists(atPath: paths.journal.path))
    }
    func assertNoSecondPipeline(file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertEqual(validator.snapshot.calls, 1, file: file, line: line)
        XCTAssertEqual(cacheChecks, 1, file: file, line: line)
        XCTAssertEqual(prepares, 1, file: file, line: line)
        XCTAssertEqual(process.requests, 1, file: file, line: line)
        XCTAssertEqual(finalizations, 1, file: file, line: line)
    }
    func attempt() async throws {
        let acknowledgment = try XCTUnwrap(session.start())
        await acknowledgment.value
        XCTAssertTrue(queue.jobs.count <= 1, "Each admitted attempt may schedule one pipeline at most")
        if !queue.jobs.isEmpty { await queue.runNext() }
    }
    func failFirstFinalization() async throws -> HvfWindowsInstallFinalizationJournal {
        try await attempt()
        guard case .failed = session.stage else { throw CocoaError(.executableRuntimeMismatch) }
        let journal = try JSONDecoder().decode(HvfWindowsInstallFinalizationJournal.self, from: Data(contentsOf: paths.journal))
        XCTAssertEqual(journal.phase, boundary == .prepared ? .prepared : .diskStaged)
        XCTAssertEqual(journal.diskSHA256, Self.digest(originalDisk)); XCTAssertEqual(journal.varsSHA256, Self.digest(originalVars))
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: plan.tmpVarsPath)), originalVars)
        XCTAssertEqual(validator.snapshot.calls, 1); XCTAssertEqual(cacheChecks, 1)
        XCTAssertEqual(prepares, 1); XCTAssertEqual(process.requests, 1); XCTAssertEqual(completions, 0)
        return journal
    }
    nonisolated static func digest(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
    nonisolated static func syntheticSeeder(_ vars: String, _ disk: String) throws -> Data {
        let receipt = HvfSecureBootProvisioningReceipt(schemaVersion: 1, policy: "test-only", sourceTag: "test",
            sourceCommit: "test", sourceAssetSha256: String(repeating: "a", count: 64), firmwareFileName: "test.fd",
            firmwareSha256: String(repeating: "b", count: 64), firmwareEdk2Commit: "test",
            provisionedAt: "2026-09-01T00:00:00Z", variables: [])
        return try JSONEncoder().encode(receipt)
    }
    func clean() {
        validator.release(); recoveryProbe.release(); queue.discard()
        try? FileManager.default.removeItem(at: root)
        for path in [plan.tmpTargetPath, plan.tmpVarsPath, plan.tmpEvidenceDir] {
            try? FileManager.default.removeItem(atPath: path)
        }
    }
}
