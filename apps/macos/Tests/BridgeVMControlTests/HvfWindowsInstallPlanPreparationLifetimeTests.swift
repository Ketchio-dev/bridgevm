import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallPlanPreparationLifetimeTests: XCTestCase {
    func testRetryUsesCurrentSnapshotAndDoesNotRewriteOtherPreparation() async throws {
        let f = try HvfWindowsInstallPlanPreparationFixture(gatedCalls: [1, 3])
        defer { f.clean() }
        try await f.withAcknowledgedWork {
            let config = f.work.config("retry", pending: true), otherConfig = f.work.config("other", pending: true)
            f.save(config); f.save(otherConfig)
            let library = f.library()
            library.operationError = "unrelated notice"
            library.selectedID = config.slug
            let preparation = try f.host(in: library).preparation
            guard await f.entered(1) else { return }
            XCTAssertTrue(f.request(diskGiB: 96).save(bundlePath: config.bundlePath))
            f.probe.release(1)
            await preparation.preparationTask?.value
            f.assertFailed(preparation)
            library.selectedID = otherConfig.slug
            let other = try f.host(in: library).preparation
            await other.preparationTask?.value
            let otherSession = try f.ready(other), otherPlan = otherSession.plan
            let oldAttempt = preparation.attemptID
            preparation.retry(); f.track(preparation)
            f.assertPreparing(preparation)
            XCTAssertNotEqual(preparation.attemptID, oldAttempt)
            guard await f.entered(3) else { return }
            library.selectedID = config.slug
            XCTAssertTrue(try f.host(in: library).preparation === preparation)
            f.probe.release(3)
            await preparation.preparationTask?.value
            let session = try f.ready(preparation)
            XCTAssertEqual(session.plan.request.diskGiB, 96)
            XCTAssertTrue(try f.ready(other) === otherSession)
            XCTAssertEqual(otherSession.plan, otherPlan)

            try FileManager.default.removeItem(at: f.work.registration(config))
            library.reload()
            f.save(config, diskGiB: 96); library.reload()
            library.selectedID = config.slug
            let replacement = try f.host(in: library).preparation
            await replacement.preparationTask?.value
            let replacementSession = try f.ready(replacement)
            XCTAssertFalse(replacement === preparation)
            XCTAssertFalse(replacementSession === session)
            let before = f.probe.snapshot.calls, made = f.made, bytes = try f.work.snapshot()
            preparation.retry() // The old unindexed view cannot claim the recreated same-slug row.
            f.track(preparation)
            f.assertFailed(preparation)
            XCTAssertTrue(f.prepare(config, in: library) === replacement)
            XCTAssertTrue(try f.ready(replacement) === replacementSession)
            XCTAssertEqual(f.probe.snapshot.calls, before)
            XCTAssertEqual(f.made, made)
            XCTAssertEqual(library.operationError, "unrelated notice")
            XCTAssertEqual(try f.work.snapshot(), bytes)
        }
    }

    func testIndependentLibraryInstancesAndRootsDoNotCoalesce() async throws {
        let first = try HvfWindowsInstallPlanPreparationFixture(gatedCalls: [1, 2])
        let second = try HvfWindowsInstallPlanPreparationFixture(gatedCalls: [1])
        defer { first.clean(); second.clean() }
        try await first.withAcknowledgedWork {
            try await second.withAcknowledgedWork {
                let a = first.work.config("same", pending: true)
                var b = second.work.config("same", pending: true)
                b.id = a.id; b.guestName = a.guestName
                first.save(a); second.save(b)
                let one = first.library(), sameRoot = first.library(), otherRoot = second.library()
                let firstHandle = try first.host(in: one).preparation
                let sameHandle = try first.host(in: sameRoot).preparation
                let otherHandle = try second.host(in: otherRoot).preparation
                guard await first.entered(1), await first.entered(2), await second.entered(1) else { return }
                XCTAssertFalse(firstHandle === sameHandle)
                XCTAssertFalse(firstHandle === otherHandle)
                let firstBytes = try first.work.snapshot(), secondBytes = try second.work.snapshot()
                first.probe.release(1); first.probe.release(2); second.probe.release(1)
                await firstHandle.preparationTask?.value
                await sameHandle.preparationTask?.value
                await otherHandle.preparationTask?.value
                let firstSession = try first.ready(firstHandle)
                let sameSession = try first.ready(sameHandle)
                let otherSession = try second.ready(otherHandle)
                XCTAssertFalse(firstSession === sameSession)
                XCTAssertFalse(firstSession === otherSession)
                XCTAssertEqual(firstSession.plan.slug, otherSession.plan.slug)
                XCTAssertEqual(firstSession.plan.libraryRoot, first.work.root)
                XCTAssertEqual(sameSession.plan.libraryRoot, first.work.root)
                XCTAssertEqual(otherSession.plan.libraryRoot, second.work.root)
                XCTAssertEqual(first.made, 2)
                XCTAssertEqual(second.made, 1)
                XCTAssertEqual(try first.work.snapshot(), firstBytes)
                XCTAssertEqual(try second.work.snapshot(), secondBytes)
            }
        }
    }

    func testOwnerReleaseAcknowledgesReadOnlyWorkerWithoutRetainingLibrary() async throws {
        for keepHandle in [false, true] {
            for cancelAcknowledgment in [false, true] {
                let f = try HvfWindowsInstallPlanPreparationFixture(gatedCalls: [1])
                defer { f.clean() }
                try await f.withAcknowledgedWork {
                    let config = f.work.config(pending: true)
                    f.save(config)
                    var library: LibraryModel? = f.library()
                    weak var observedLibrary = library
                    var preparation: HvfWindowsInstallPreparation? = try f.host(in: XCTUnwrap(library)).preparation
                    weak var observedPreparation = preparation
                    let task = try XCTUnwrap(preparation?.preparationTask)
                    guard await f.entered(1) else { return }
                    let bytes = try f.work.snapshot()
                    if !keepHandle { preparation = nil }
                    library = nil
                    XCTAssertNil(observedLibrary, "The owner must release before the read-only worker is unblocked")
                    if !keepHandle { XCTAssertNil(observedPreparation) }
                    if cancelAcknowledgment { task.cancel() }
                    XCTAssertEqual(f.probe.snapshot.finished, 0)
                    f.probe.release(1)
                    await task.value
                    XCTAssertEqual(f.probe.snapshot.finished, 1)
                    XCTAssertEqual(f.made, 0, "An expired owner must not publish a session")
                    if let preparation { f.assertFailed(preparation) }
                    preparation = nil
                    XCTAssertNil(observedPreparation)
                    XCTAssertEqual(try f.work.snapshot(), bytes)
                }
            }
        }
    }
}
