import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeRetainedControlRoutingTests: XCTestCase {
    func testMissingSelectedRuntimeRoutesToExactRetainedControl() throws {
        for state in HvfRuntimeWorkAdmissionFixture.states {
            for hasOtherRow in [false, true] {
                for proMode in [false, true] {
                    let f = try HvfRuntimeRetainedControlFixture()
                    defer { f.clean() }
                    let config = f.work.config("lost"), other = f.work.config("other")
                    f.work.save(config)
                    if hasOtherRow { f.work.save(other) }
                    let library = f.work.library()
                    library.selectedID = config.slug
                    library.proMode = proMode
                    let session = try f.work.runtime(config, in: library)
                    session.connectionState = state // Observed state only; no Start or attachment.
                    session.events = [.unknown("accepted runtime observation")]
                    session.lastHeartbeatAge = 9
                    let before = f.effects
                    try f.removeRegistration(config)
                    let bytes = try f.work.snapshot()

                    library.reload()
                    XCTAssertEqual(library.vms.count, hasOtherRow ? 1 : 0)
                    XCTAssertNotEqual(library.selectedID, config.slug)
                    // Construct the actual body even when the missing feature has produced no record.
                    let token = f.assertRuntimeRoute(session, config: config, in: library)
                    XCTAssertEqual(f.effects, before)
                    XCTAssertEqual(try f.work.snapshot(), bytes)
                    guard let token else { continue }
                    XCTAssertNotEqual(token, config.slug)
                    XCTAssertNotEqual(token, LibraryModel.firstRunImportSelectionID)
                    XCTAssertNotEqual(token, LibraryModel.hvfEngineSelectionID)
                    XCTAssertEqual(library.retainedControlRecords.count, 1)
                    XCTAssertEqual(session.connectionState, state)
                    XCTAssertEqual(session.events, [.unknown("accepted runtime observation")])
                    XCTAssertEqual(session.lastHeartbeatAge, 9)
                    library.reload()
                    XCTAssertEqual(f.assertRuntimeRoute(session, config: config, in: library), token)
                    XCTAssertEqual(library.retainedControlRecords.count, 1)
                    XCTAssertEqual(f.effects, before)
                    XCTAssertEqual(try f.work.snapshot(), bytes)
                }
            }
        }
    }

    func testMissingInstallRoutesThroughValidationAndQueuedCancellation() async throws {
        do {
            let f = try HvfRuntimeRetainedControlFixture()
            defer { f.clean() }
            let config = f.work.config(pending: true)
            f.work.save(config); f.work.saveRequest(config)
            let library = f.work.library()
            library.selectedID = config.slug
            var token: String?
            let session = try await f.withValidatingInstall(config, library: library) { session in
                XCTAssertEqual(session.stage, .validating)
                try f.removeRegistration(config)
                let bytes = try f.work.snapshot(), before = f.effects
                library.reload()
                token = f.assertInstallRoute(session, config: config, in: library)
                XCTAssertEqual(f.effects, before)
                session.cancel()
                XCTAssertTrue(session.isRunning)
                XCTAssertEqual(f.assertInstallRoute(session, config: config, in: library), token)
                XCTAssertEqual(try f.work.snapshot(), bytes)
            }
            XCTAssertEqual(session.stage, .failed("설치가 취소되었습니다."))
            XCTAssertEqual(f.work.installJobCount, 0)
            guard let token else { return XCTFail("The captured install must have an exact selection token") }
            XCTAssertEqual(library.selectedID, token)
            XCTAssertEqual(f.assertInstallRoute(session, config: config, in: library), token)
            library.reload() // The ordinary store may prune; the terminal retained detail remains.
            XCTAssertEqual(f.assertInstallRoute(session, config: config, in: library), token)
            XCTAssertTrue(session.logLines.contains("사용자가 설치를 취소했습니다."))
        }
        do {
            let f = try HvfRuntimeRetainedControlFixture(validator: .init())
            defer { f.clean() }
            let config = f.work.config(pending: true)
            f.work.save(config); f.work.saveRequest(config)
            let library = f.work.library()
            let session = f.work.install(config, in: library)
            let handle = session.start()
            XCTAssertNotNil(handle)
            if let handle { await handle.value }
            XCTAssertEqual(session.stage, .preparingSource)
            XCTAssertEqual(f.work.installJobCount, 1)
            library.selectedID = config.slug
            try f.removeRegistration(config)
            let bytes = try f.work.snapshot()
            library.reload()
            guard let token = f.assertInstallRoute(session, config: config, in: library) else { return }
            session.cancel()
            XCTAssertEqual(session.stage, .preparingSource)
            XCTAssertTrue(session.isRunning)
            XCTAssertFalse(library.dismissRetainedControl(token))
            XCTAssertEqual(f.assertInstallRoute(session, config: config, in: library), token)
            XCTAssertEqual(f.work.installJobCount, 1)
            XCTAssertEqual(try f.work.snapshot(), bytes)
            // The queued guard is never executed; pending cancellation is not terminal completion.
        }
    }

    func testDistinctSlugsAndBothKindsHaveIndependentTokens() async throws {
        let f = try HvfRuntimeRetainedControlFixture()
        defer { f.clean() }
        let installConfig = f.work.config("shared", pending: true), otherConfig = f.work.config("other")
        f.work.save(installConfig); f.work.saveRequest(installConfig); f.work.save(otherConfig)
        let library = f.work.library()
        library.selectedID = installConfig.slug
        _ = try await f.withValidatingInstall(installConfig, library: library) { installer in
            var runtimeConfig = installConfig
            runtimeConfig.installPending = false
            runtimeConfig.displayName = "accepted runtime metadata"
            f.work.save(runtimeConfig); library.reload()
            let runtime = try f.work.runtime(runtimeConfig, in: library)
            let other = try f.work.runtime(otherConfig, in: library)
            // Simulate two existing observations; B is never bypassed to launch competing work.
            runtime.connectionState = .booting
            other.connectionState = .timedOut
            try f.removeRegistration(runtimeConfig); try f.removeRegistration(otherConfig)
            let bytes = try f.work.snapshot(), before = f.effects
            library.reload()
            XCTAssertEqual(library.retainedControlRecords.count, 3)
            let installToken = try XCTUnwrap(f.installRecord(installer, in: library)?.id)
            let runtimeToken = try XCTUnwrap(f.runtimeRecord(runtime, in: library)?.id)
            let otherToken = try XCTUnwrap(f.runtimeRecord(other, in: library)?.id)
            XCTAssertEqual(Set([installToken, runtimeToken, otherToken]).count, 3)
            XCTAssertEqual(library.selectedID, installToken, "A disappeared selected slug keeps install-first priority")
            f.assertInstallRoute(installer, config: installConfig, in: library)
            library.selectedID = runtimeToken
            f.assertRuntimeRoute(runtime, config: runtimeConfig, in: library)
            library.selectedID = otherToken
            f.assertRuntimeRoute(other, config: otherConfig, in: library)
            library.reload()
            XCTAssertEqual(library.selectedID, otherToken)
            XCTAssertEqual(Set(library.retainedControlRecords.map(\.id)), Set([installToken, runtimeToken, otherToken]))
            XCTAssertEqual(f.effects, before)
            XCTAssertEqual(try f.work.snapshot(), bytes)
            installer.cancel()
        }
        XCTAssertEqual(f.work.installJobCount, 0)
    }

    func testNavigationSentinelsAndInactiveRemovalKeepExistingRoutes() throws {
        let f = try HvfRuntimeRetainedControlFixture()
        defer { f.clean() }
        let lost = f.work.config("lost"), normal = f.work.config("normal")
        f.work.save(lost); f.work.save(normal)
        let library = f.work.library()
        library.selectedID = lost.slug
        let retained = try f.work.runtime(lost, in: library)
        retained.connectionState = .connected(host: "fixture")
        try f.removeRegistration(lost); library.reload()
        guard let token = f.assertRuntimeRoute(retained, config: lost, in: library) else { return }
        library.selectedID = normal.slug
        let ordinary = LibraryDetailView(library: library).body
        XCTAssertEqual(f.values(HvfEngineView.self, in: ordinary).count, 1)
        XCTAssertTrue(f.values(RetainedControlDetailView.self, in: ordinary).isEmpty)
        let before = f.effects
        library.proMode = true
        library.selectedID = token
        f.assertRuntimeRoute(retained, config: lost, in: library)
        XCTAssertTrue(library.proMode)
        XCTAssertEqual(f.effects, before)

        library.selectedID = LibraryModel.firstRunImportSelectionID
        library.reload()
        XCTAssertEqual(library.selectedID, LibraryModel.firstRunImportSelectionID)
        XCTAssertEqual(f.values(FirstRunView.self, in: LibraryDetailView(library: library).body).count, 1)
        library.proMode = false
        library.selectedID = LibraryModel.hvfEngineSelectionID
        library.reload()
        XCTAssertEqual(library.selectedID, LibraryModel.hvfEngineSelectionID)
        XCTAssertEqual(f.values(HvfEngineView.self, in: LibraryDetailView(library: library).body).count, 1)
        XCTAssertEqual(library.retainedControlRecords.count, 1, "Experimental selection is never captured")

        let inactive = f.work.config("inactive")
        f.work.save(inactive); library.reload()
        library.selectedID = inactive.slug
        let idle = try f.work.runtime(inactive, in: library)
        XCTAssertEqual(idle.connectionState, .stopped)
        try f.removeRegistration(inactive); library.reload()
        XCTAssertEqual(library.selectedID, library.vms.first?.slug)
        XCTAssertNil(library.selectedRetainedControl)
        XCTAssertEqual(library.retainedControlRecords.count, 1)
        library.selectedID = token
        f.assertRuntimeRoute(retained, config: lost, in: library)
    }
}
