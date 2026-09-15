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
            let model = f.model(config, in: library)
            await f.assertControlRefused(model, operation: .resources, library: library,
                reason: HvfRuntimeWorkAdmissionFixture.runtimeReason)

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
            let currentModel = f.model(changedBackend, in: library)
            await f.assertControlRefused(currentModel, operation: .resources, library: library,
                reason: HvfRuntimeWorkAdmissionFixture.runtimeReason)
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
        let model = f.model(changed, in: library)
        await f.assertControlRefused(model, operation: .resources, library: library,
            reason: HvfRuntimeWorkAdmissionFixture.installReason)
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
        let model = f.model(changed, in: library)
        let bytes = try f.snapshot()

        f.assertRuntimeRefused(runtime, library: library, reason: HvfRuntimeWorkAdmissionFixture.installReason)
        await f.assertControlRefused(model, operation: .resources, library: library,
            reason: HvfRuntimeWorkAdmissionFixture.installReason)
        installer.cancel()
        XCTAssertTrue(installer.isRunning)
        XCTAssertEqual(installer.stage, .cancelling)
        XCTAssertTrue(installer.logLines.contains("사용자가 설치를 취소했습니다."))
        f.assertRuntimeRefused(runtime, library: library, reason: HvfRuntimeWorkAdmissionFixture.installReason,
            entries: [.automaticAttach, .silentAttach])
        XCTAssertEqual(f.installJobCount, 1)
        XCTAssertEqual(try f.snapshot(), bytes)
        // The captured pipeline is discarded; this test does not claim cancellation completion.
    }

    func testActualGenericWorkAndConfirmationOwnAdmissionButObservedRunningDoesNot() async throws {
        for operation in HvfRuntimeWorkAdmissionBackend.Operation.allCases {
            for target in ["runtime", "install"] {
                let f = try HvfRuntimeWorkAdmissionFixture(acceptsFakeStart: true)
                defer { f.clean() }
                let config = f.config(pending: target == "install")
                f.save(config)
                if target == "install" { f.saveRequest(config) }
                let library = f.library()
                let runtime = target == "runtime" ? try f.runtime(config, in: library) : nil
                let installer = target == "install" ? f.install(config, in: library) : nil
                let model = f.model(config, in: library), backend = f.backend(config)
                let work = HvfRuntimeWorkAdmissionOperation(operation, model: model)
                backend.hold(operation)
                let accepted = work.invoke()
                XCTAssertTrue(accepted)
                guard accepted else { continue }
                let entered = await backend.waitForEntry()
                XCTAssertTrue(entered)
                XCTAssertTrue(model.hasAcceptedOperation)
                if let runtime {
                    f.assertRuntimeRefused(runtime, library: library, reason: HvfRuntimeWorkAdmissionFixture.controlReason)
                }
                if let installer {
                    await f.assertInstallRefused(installer, library: library, reason: HvfRuntimeWorkAdmissionFixture.controlReason)
                }
                backend.releaseGate()
                guard await f.acknowledge(work, accepted: accepted) else { continue }
                XCTAssertFalse(backend.gateTimedOut)
                if operation == .start {
                    XCTAssertFalse(model.lifecycleBusy)
                    XCTAssertTrue(model.hasAcceptedOperation, "Successful fake Start awaits confirmation")
                    if let runtime {
                        f.assertRuntimeRefused(runtime, library: library,
                            reason: HvfRuntimeWorkAdmissionFixture.controlReason, entries: [.configuration, .attach])
                    }
                    if let installer {
                        await f.assertInstallRefused(installer, library: library,
                            reason: HvfRuntimeWorkAdmissionFixture.controlReason)
                    }
                    await f.assertControlRefused(model, operation: .resources, library: library,
                        reason: HvfRuntimeWorkAdmissionFixture.controlReason)
                }
            }
        }

        let f = try HvfRuntimeWorkAdmissionFixture()
        defer { f.clean() }
        let config = f.config()
        f.save(config)
        let library = f.library()
        let model = f.model(config, in: library)
        model.running = true // Observation alone; no fake Start has been accepted.
        XCTAssertFalse(model.hasAcceptedOperation)
        let runtime = try f.runtime(config, in: library)
        XCTAssertTrue(runtime.acceptStartConfiguration(runtime.config))
        XCTAssertFalse(runtime.attachToRunningVM())
        XCTAssertEqual(f.access.processLookups, 1)
        XCTAssertNil(library.operationError)
        let resources = HvfRuntimeWorkAdmissionOperation(.resources, model: model)
        let accepted = resources.invoke()
        XCTAssertTrue(accepted)
        guard await f.acknowledge(resources, accepted: accepted) else { return }
        XCTAssertEqual(f.backend(config).calls.writes, 1)
        XCTAssertEqual(f.backend(config).calls.starts, 0)
        XCTAssertNil(library.operationError)
    }
}
