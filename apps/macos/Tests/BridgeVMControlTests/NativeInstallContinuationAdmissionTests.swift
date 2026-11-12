import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class NativeInstallContinuationAdmissionTests: XCTestCase {
    private final class ProcessRecorder: HvfWindowsInstallProcessRunning {
        var launches = 0
        func reset() {}
        func cancel() {}
        func run(plan: HvfWindowsInstallPlan, arguments: [String], environment: [String: String],
                 progressLog: URL?, output: @escaping (String) -> Void) async -> Bool {
            launches += 1
            XCTAssertTrue(arguments.contains("scripts/run-hvf-windows-scripted-install.sh"))
            return false // Record launch only; no real process or publication.
        }
    }

    private func run(_ conflict: String) async throws {
        let f = HvfWindowsInstallStoreFixture()
        defer { f.clean() }
        let saved = f.config()
        try f.save(saved)
        let queue = HvfWindowsInstallPipelineQueue()
        defer { queue.discard() }
        let process = ProcessRecorder()
        var prepared = 0
        var session: HvfWindowsInstallSession?
        var library: LibraryModel!
        library = LibraryModel(rootURL: f.root, migrateLegacy: false, installSessionFactory: { plan in
            let value = HvfWindowsInstallSession(plan: plan, validate: { _ in nil }, schedule: queue.enqueue,
                execution: .init(process: process, verifySource: { _ in true }, prepareMedia: { _ in
                    prepared += 1
                    XCTAssertEqual(library.vms.first { $0.slug == saved.slug }, saved,
                                   "media preparation must not mutate the admitted registration")
                    if conflict == "cli" {
                        let other = NativeInstallOperation(operationID: UUID(), expectedSavedConfigurationDigest: "other")
                        library.windowsInstallSessions.cliOperations.records[saved.slug] = .init(digest: "other", operation: other)
                    } else if conflict == "config" {
                        var changed = saved; changed.memMiB = 8192
                        try f.save(changed); library.reload()
                    }
                }), recovery: .init(inspect: { _ in .fresh }))
            session = value
            return value
        })
        let handle = try NativeRuntimeLibraryHandle.open(rootURL: f.root, create: false)
        let digest = try NativeRuntimeConfigurationIdentity.digest(config: saved)
        let request = NativeInstallControlRequest(schema: NativeInstallControlCodec.requestSchema, operation: .install,
            requestID: UUID().uuidString, library: handle.identity, vmID: saved.slug,
            appInstanceID: UUID().uuidString, expectedSavedConfigurationDigest: digest, operationID: UUID().uuidString)
        let accepted = try library.requestOwnedInstall(request, validateOwner: {})
        for _ in 0..<500 where queue.jobs.isEmpty { try await Task.sleep(nanoseconds: 1_000_000) }
        let active = try XCTUnwrap(session)
        XCTAssertTrue(accepted.operation.reservesWork)
        XCTAssertEqual(queue.jobs.count, 1)
        if conflict == "gui" {
            // A GUI-bound session has no authority to exclude the active CLI owner.
            _ = library.bindWindowsInstallSession(active, slug: saved.slug)
        }
        await queue.runNext()
        XCTAssertEqual(prepared, 1)
        if conflict.isEmpty {
            XCTAssertEqual(process.launches, 1, "owning CLI operation must reach installer launch after preparation; stage=\(active.stage)")
        } else {
            XCTAssertEqual(process.launches, 0, "foreign work or changed registration must stay refused")
            guard case .failed = active.stage else { return XCTFail("expected admission refusal, got \(active.stage)") }
        }
    }

    func testOwnedInstallContinuationExcludesItsOwnSessionAndOperation() async throws { try await run("") }
    func testOwnedInstallContinuationRejectsDifferentCLIOperation() async throws { try await run("cli") }
    func testGUIContinuationDoesNotExcludeCLIWork() async throws { try await run("gui") }
    func testOwnedInstallContinuationRejectsChangedRegistration() async throws { try await run("config") }
}
