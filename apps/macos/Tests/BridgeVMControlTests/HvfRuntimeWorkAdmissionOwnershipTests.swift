import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeWorkAdmissionOwnershipTests: XCTestCase {
    func testEveryNonStoppedRuntimeProtectsItselfAndOtherEntryKinds() async throws {
        for state in HvfRuntimeWorkAdmissionFixture.states {
            let f = try HvfRuntimeWorkAdmissionFixture()
            defer { f.clean() }
            let config = f.config()
            f.save(config)
            let library = f.library()
            let runtime = try f.runtime(config, in: library)
            for entry in HvfRuntimeWorkAdmissionEntry.allCases {
                runtime.connectionState = state
                runtime.events = [.unknown("accepted runtime observation")]
                runtime.lastHeartbeatAge = 12
                f.assertRuntimeRefused(runtime, library: library,
                    reason: HvfRuntimeWorkAdmissionFixture.runtimeReason, entries: [entry])
            }
            runtime.connectionState = state

            var pending = config
            pending.installPending = true
            f.save(pending); f.saveRequest(pending); library.reload()
            XCTAssertTrue(library.hvfRuntimeSession(for: pending) === runtime)
            let installer = f.install(pending, in: library)
            await f.assertInstallRefused(installer, library: library,
                reason: HvfRuntimeWorkAdmissionFixture.runtimeReason)

            var changedBackend = pending
            changedBackend.backendKind = "fast-vz"
            XCTAssertNotEqual(changedBackend.engineKind, .hvfEngine)
            f.save(changedBackend); library.reload()
            XCTAssertTrue(library.hvfRuntimeSession(for: changedBackend) === runtime)
            XCTAssertEqual(runtime.connectionState, state)
            XCTAssertEqual(f.installJobCount, 0)
        }
    }

    func testValidatingInstallRetainsOwnershipAcrossMetadataAndCancellationAcknowledgement() async throws {
        let probe = HvfWindowsInstallValidationProbe(error: "owned validation refusal", gateFirstCall: true)
        let f = try HvfRuntimeWorkAdmissionFixture(validator: probe)
        defer { probe.release(); f.clean() }
        let original = f.config(pending: true)
        f.save(original); f.saveRequest(original)
        let library = f.library()
        let installer = f.install(original, in: library)
        let acceptedPlan = installer.plan
        guard let handle = installer.start() else { return XCTFail("Fixture validation must be admitted") }
        let entered = await probe.waitForEntry()
        XCTAssertTrue(entered)
        var changed = original
        changed.installPending = false; changed.name = "current metadata"
        f.save(changed); library.reload()
        XCTAssertTrue(f.install(original, in: library) === installer)
        XCTAssertEqual(installer.plan, acceptedPlan)
        // Nothing throwing occurs while the finite validation gate owns an acknowledgement.
        if let runtime = library.hvfRuntimeSession(for: changed) {
            f.assertRuntimeRefused(runtime, library: library, reason: HvfRuntimeWorkAdmissionFixture.installReason)
        } else { XCTFail("Current non-pending metadata must produce the safe runtime fixture") }
        installer.cancel()
        XCTAssertTrue(installer.isRunning, "Cancellation remains owned until validation acknowledges")
        let duplicate = installer.start()
        XCTAssertNil(duplicate)
        probe.release()
        await handle.value
        if let duplicate { await duplicate.value }
        XCTAssertEqual(installer.stage, .cancelled)
        XCTAssertFalse(probe.snapshot.gateTimedOut)
        XCTAssertEqual(f.installJobCount, 0)

        changed.installPending = true
        f.save(changed); library.reload()
        let fresh = f.install(changed, in: library)
        XCTAssertFalse(fresh === installer)
        XCTAssertTrue(f.install(original, in: library) === fresh)
        await f.assertInstallRefused(installer, library: library, reason: HvfRuntimeWorkAdmissionFixture.ownerReason)
        let retried = fresh.start()
        XCTAssertNotNil(retried)
        if let retried { await retried.value }
        XCTAssertEqual(fresh.stage, .failed("owned validation refusal"))
    }

    func testQueuedInstallAndPendingCancellationKeepTheirReservationWithoutRunningPipeline() async throws {
        let f = try HvfRuntimeWorkAdmissionFixture(validator: .init())
        defer { f.clean() }
        let config = f.config(pending: true)
        f.save(config); f.saveRequest(config)
        let library = f.library()
        let installer = f.install(config, in: library)
        let handle = installer.start()
        XCTAssertNotNil(handle)
        if let handle { await handle.value }
        XCTAssertEqual(installer.stage, .preparingSource)
        XCTAssertEqual(f.installJobCount, 1)
        var changed = config
        changed.installPending = false
        f.save(changed); library.reload()
        let runtime = try f.runtime(changed, in: library)
        let bytes = try f.snapshot()

        f.assertRuntimeRefused(runtime, library: library, reason: HvfRuntimeWorkAdmissionFixture.installReason)
        installer.cancel()
        XCTAssertTrue(installer.isRunning)
        XCTAssertEqual(installer.stage, .cancelling)
        XCTAssertTrue(installer.logLines.contains("Installation canceled by user."))
        f.assertRuntimeRefused(runtime, library: library, reason: HvfRuntimeWorkAdmissionFixture.installReason,
            entries: [.automaticAttach, .silentAttach])
        XCTAssertEqual(f.installJobCount, 1)
        XCTAssertEqual(try f.snapshot(), bytes)
        // The captured pipeline is discarded; this test does not claim cancellation completion.
    }

    func testObservedRunningDoesNotOwnAdmission() throws {
        let f = try HvfRuntimeWorkAdmissionFixture()
        defer { f.clean() }
        let config = f.config()
        f.save(config)
        let library = f.library()
        let model = f.model(config, in: library)
        model.running = true // Observation alone: the library's poll saw a process on the disk.
        let runtime = try f.runtime(config, in: library)
        XCTAssertTrue(runtime.acceptStartConfiguration(runtime.config))
        XCTAssertFalse(runtime.attachToRunningVM())
        XCTAssertEqual(f.access.processLookups, 1)
        XCTAssertNil(library.operationError)
    }
}
