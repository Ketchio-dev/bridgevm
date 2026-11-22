import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryModelOwnedRuntimeStartTests: XCTestCase {
    func testMissingPendingInstallAndOtherBackendRefuseWithASentenceBeforeRuntimeFactory() throws {
        for pending in [false, true] {
            let f = try HvfRuntimeWorkAdmissionFixture(); defer { f.clean() }
            var config = f.config(pending: pending)
            if !pending { config.backendKind = "fast-vz" }
            f.save(config); let model = f.library(); let before = try f.snapshot()
            for id in [config.slug, "missing"] {
                guard case let .refused(reason) = model.requestCLIStart(vmID: id) else { return XCTFail("\(id) was admitted") }
                XCTAssertFalse(reason.isEmpty)
            }
            XCTAssertEqual(f.runtimeCreations, 0); XCTAssertEqual(f.access.keyRequests, 0)
            XCTAssertEqual(try f.snapshot(), before)
        }
    }

    /// The CLI used to send a digest of the vm.json it read, and the app refused with
    /// `configurationChanged` when the file changed in between. Now the app starts what is saved.
    func testStartsTheCurrentSavedConfigurationAfterVMJsonChangedOnDisk() async throws {
        let f = try HvfRuntimeWorkAdmissionFixture(); defer { f.clean() }
        let config = f.config(); f.save(config)
        var session: HvfEngineSession?
        let model = LibraryModel(rootURL: f.root, migrateLegacy: false, runtimeSessionFactory: { launch in
            let value = HvfEngineSession(config: launch, repoRoot: f.runtimeRepo,
                processIsRunning: { _ in false }, vtpmKeyProvider: f.access)
            value.guiStartWorker = { _ in .init(result: .failed(.processLaunch, "fixture runner is absent")) }
            session = value; return value
        }, startsModelsAutomatically: false)
        model.reload()
        var changed = config; changed.memMiB = 8192; f.save(changed)
        guard case let .accepted(ticket, started) = model.requestCLIStart(vmID: config.slug) else {
            return XCTFail("CLI start of the saved configuration was refused")
        }
        XCTAssertTrue(started === session)
        XCTAssertEqual(ticket.configuration.ramMiB, 8192)
        let deadline = ProcessInfo.processInfo.systemUptime + 2
        while ticket.workerPending, ProcessInfo.processInfo.systemUptime < deadline { try await Task.sleep(nanoseconds: 5_000_000) }
        // The start path's own sentence, with the real error text, is what the CLI receives.
        XCTAssertEqual(ticket.outcome?.failureText?.hasSuffix("fixture runner is absent"), true)
    }

    func testRetainedRuntimeWorkRefusesTheCLIStartLikeTheStartButton() throws {
        let f = try HvfRuntimeWorkAdmissionFixture(); defer { f.clean() }
        let config = f.config(); f.save(config); let model = f.library()
        let session = try f.runtime(config, in: model)
        let reservation = try XCTUnwrap(session.reserveRuntimeMutation(kind: .snapshot))
        defer { session.finishRuntimeMutation(reservation) }
        guard case .refused = model.requestCLIStart(vmID: config.slug) else { return XCTFail("Start ran during a mutation") }
        XCTAssertEqual(f.access.processLookups, 0); XCTAssertEqual(f.access.keyRequests, 0)
    }
}
