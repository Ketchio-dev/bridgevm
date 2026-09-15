import XCTest
import Combine
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallPlanPreparationRoutingTests: XCTestCase {
    func testActualPendingDetailBuildsOwnedPlanOffMainForSealedAndUnsealedRequests() async throws {
        for sealed in [true, false] {
            let f = try HvfWindowsInstallPlanPreparationFixture()
            defer { f.clean() }
            try await f.withAcknowledgedWork {
                let config = f.work.config(pending: true)
                f.save(config, sealed: sealed)
                let library = f.library()
                library.selectedID = config.slug
                let bytes = try f.work.snapshot()
                let host = try f.host(in: library) // Actual product route; no mounted view or fake activation.
                await host.preparation.preparationTask?.value // The synchronous baseline has no Task.
                let session = try f.ready(host.preparation)
                let expected = HvfWindowsInstallPlan(repoRoot: f.work.installRepo, libraryRoot: f.work.root,
                    bundlePath: config.bundlePath, slug: config.slug, request: f.request(sealed: sealed))
                XCTAssertEqual(f.probe.snapshot.calls, 1)
                XCTAssertEqual(f.probe.snapshot.threadMain, [false], "The actual detail's builder must leave main")
                XCTAssertEqual(f.probe.snapshot.invalidInputs, 0)
                XCTAssertEqual(session.plan, expected)
                XCTAssertEqual(session.plan.sealedISOSHA256, f.request().isoSHA256)
                XCTAssertEqual(session.stage, .idle)
                XCTAssertEqual(f.work.validator.snapshot.calls, 0)
                XCTAssertEqual(f.jobCount, 0)
                XCTAssertEqual(f.made, 1)
                try f.assertReadyHost(host, session: session)
                XCTAssertEqual(try f.work.snapshot(), bytes)
            }
        }
    }

    func testGatedPreparationLeavesMainActorFreeAndCoalescesNavigation() async throws {
        let f = try HvfWindowsInstallPlanPreparationFixture(gatedCalls: [1, 2, 3])
        defer { f.clean() }
        try await f.withAcknowledgedWork {
            let a = f.work.config("a", pending: true), b = f.work.config("b", pending: true)
            f.save(a); f.save(b)
            let library = f.library()
            library.selectedID = a.slug
            let first = try f.host(in: library)
            f.assertPreparing(first.preparation)
            XCTAssertTrue(f.values(HvfWindowsInstallView.self, in: first.body).isEmpty)
            guard await f.entered(1) else { return }
            let firstTask = try XCTUnwrap(first.preparation.preparationTask)
            let actorProgress = await Task { @MainActor in Thread.isMainThread }.value
            XCTAssertTrue(actorProgress)
            XCTAssertEqual(f.probe.snapshot.finished, 0)
            library.selectedID = b.slug
            let other = try f.host(in: library)
            guard await f.entered(2) else { return }
            library.selectedID = a.slug
            XCTAssertTrue(try f.host(in: library).preparation === first.preparation)
            XCTAssertEqual(f.probe.snapshot.calls, 2)

            var oldPublications = 0
            let oldSubscription = first.preparation.objectWillChange.sink { oldPublications += 1 }
            defer { oldSubscription.cancel() }
            XCTAssertTrue(f.request(diskGiB: 96).save(bundlePath: a.bundlePath)) // No reload.
            let bytes = try f.work.snapshot()
            let replacement = try f.host(in: library)
            XCTAssertFalse(replacement.preparation === first.preparation)
            f.assertPreparing(replacement.preparation)
            XCTAssertEqual(oldPublications, 0, "Body-time lookup must not publish through the old handle")
            var newPublications = 0
            let newSubscription = replacement.preparation.objectWillChange.sink { newPublications += 1 }
            defer { newSubscription.cancel() }
            XCTAssertTrue(try f.host(in: library).preparation === replacement.preparation)
            XCTAssertEqual(newPublications, 0)
            XCTAssertEqual(oldPublications, 0)
            guard await f.entered(3) else { return }
            XCTAssertEqual(f.probe.snapshot.calls, 3)
            XCTAssertEqual(f.probe.snapshot.threadMain, [false, false, false])
            XCTAssertEqual(f.probe.snapshot.finished, 0)
            let currentTask = try XCTUnwrap(replacement.preparation.preparationTask)
            currentTask.cancel() // Cancelling acknowledgment does not cancel the detached read.
            XCTAssertTrue(currentTask.isCancelled)
            f.assertPreparing(replacement.preparation)
            f.probe.release(3)
            await currentTask.value
            let current = try f.ready(replacement.preparation)
            XCTAssertEqual(current.plan.request.diskGiB, 96)
            f.probe.release(2)
            await other.preparation.preparationTask?.value
            XCTAssertEqual(try f.ready(other.preparation).plan.slug, b.slug)
            f.probe.release(1)
            await firstTask.value
            XCTAssertTrue(try f.host(in: library).preparation === replacement.preparation)
            XCTAssertTrue(try f.ready(replacement.preparation) === current)
            XCTAssertEqual(f.made, 2)
            XCTAssertEqual(try f.work.snapshot(), bytes)
        }
    }

    func testPreparedAndActiveSessionsWinBeforeNewPlanWork() async throws {
        for mode in ["idle", "validating", "queued"] {
            let f = try HvfWindowsInstallPlanPreparationFixture(queuedInstall: mode == "queued")
            defer { f.clean() }
            try await f.withAcknowledgedWork {
                let original = f.work.config(pending: true)
                f.save(original)
                let library = f.library()
                let host = try f.host(in: library)
                await host.preparation.preparationTask?.value
                let session = try f.ready(host.preparation), plan = session.plan
                let repeated = try f.host(in: library)
                XCTAssertTrue(repeated.preparation === host.preparation)
                try f.assertReadyHost(repeated, session: session)
                if mode != "idle" {
                    try await f.withActive(session, queued: mode == "queued") {
                        var changed = original
                        changed.displayName = "changed registration while install owns work"
                        changed.bundlePath = f.work.root.appendingPathComponent("replacement/bundle").path
                        changed.backendKind = "fast-vz"
                        changed.installPending = false
                        f.save(changed, diskGiB: 96)
                        library.reload()
                        let bytes = try f.work.snapshot()
                        let retained = try f.host(in: library)
                        try f.assertReadyHost(retained, session: session) // Immediate; no second await.
                        XCTAssertTrue(try f.ready(f.prepare(original, in: library)) === session)
                        XCTAssertEqual(session.plan, plan)
                        XCTAssertTrue(session.isRunning)
                        XCTAssertEqual(try f.work.snapshot(), bytes)
                    }
                }
                XCTAssertEqual(f.probe.snapshot.calls, 1)
                XCTAssertEqual(f.made, 1)
                XCTAssertEqual(f.jobCount, mode == "queued" ? 1 : 0)
                XCTAssertEqual(f.work.runtimeCreations, 0)
            }
        }
    }
}
