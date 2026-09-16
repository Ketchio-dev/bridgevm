import Combine
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfGUIStartTests: XCTestCase {
    func testReservationPrecedesEditedConfigurationPublicationAndBlocksOtherEntrypoints() async throws {
        let gate = HvfGUIStartGate()
        let f = try HvfGUIStartFixture(rejectLaunch: true, probeGate: gate)
        do {
            var callbacks = 0
            let observer = f.session.$config.dropFirst().sink { _ in
                callbacks += 1
                XCTAssertTrue(f.session.hasPendingGUIStart)
                XCTAssertNil(f.session.reserveRuntimeMutation(kind: .snapshot))
                XCTAssertFalse(f.session.acceptStartConfiguration(f.base.config))
                if case .refused = f.session.start() {} else { XCTFail("Synchronous start reentered") }
                if case .refused = f.session.requestGUIStart(configuration: f.base.config) {} else { XCTFail("GUI start reentered") }
                let cli = f.session.requestOwnedStart(configuration: f.base.config, operationID: UUID(),
                    expectedSavedConfigurationDigest: HvfOwnedStartTestSupport.digest,
                    onReserved: { _ in XCTFail("CLI must refuse before registration") }, revalidate: { true })
                if case .refused = cli {} else { XCTFail("CLI start overlaps GUI reservation") }
            }
            defer { observer.cancel() }
            var edited = f.base.config; edited.ramMiB = 2048; edited.smpCpus = 2
            let ticket = try f.start(configuration: edited)
            XCTAssertEqual(ticket.configuration, edited); XCTAssertEqual(f.session.config, edited)
            XCTAssertTrue(callbacks > 0); XCTAssertTrue(ticket.workerPending)
            try await f.wait { gate.state.entered }
            XCTAssertTrue(f.session.hasActiveRuntimeWork); XCTAssertEqual(f.session.connectionState, .stopped)
            gate.release(); try await f.wait { !ticket.workerPending }
            XCTAssertFalse(gate.state.expired); XCTAssertEqual(f.effects.snapshot.launches, 1)
            XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: edited.evidenceDir).appendingPathComponent("launch-manifest.json")), Data(edited.launchManifestJSON().utf8))
        } catch { await f.clean(); throw error }
        await f.clean()
    }

    func testBlockedInteractiveKeyLookupLeavesMainActorAvailableAndOwnsReservation() async throws {
        let gate = HvfGUIStartGate()
        let f = try HvfGUIStartFixture(encrypted: true, rejectLaunch: true, keyGate: gate)
        do {
            let ticket = try f.start()
            try await f.wait { gate.state.entered }
            var sentinel = false
            await Task { @MainActor in sentinel = true }.value
            XCTAssertTrue(sentinel); XCTAssertFalse(gate.state.expired)
            XCTAssertTrue(ticket.workerPending); XCTAssertTrue(f.session.hasActiveRuntimeWork)
            XCTAssertNil(f.session.reserveRuntimeMutation(kind: .vtpmReset))
            XCTAssertEqual(f.effects.snapshot.launches, 0)
            XCTAssertTrue(f.effects.snapshot.offMain.allSatisfy { $0 })
            gate.release(); try await f.wait { !ticket.workerPending }
            XCTAssertEqual(f.effects.snapshot.keyCreation, [true]); XCTAssertEqual(f.effects.snapshot.existingReads, 0)
        } catch { await f.clean(); throw error }
        await f.clean()
    }

    func testInvalidationWhileProbeHeldNeverBeginsPreparationKeyOrSpawn() async throws {
        let gate = HvfGUIStartGate()
        let f = try HvfGUIStartFixture(encrypted: true, probeGate: gate)
        do {
            let ticket = try f.start()
            try await f.wait { gate.state.entered }
            let original = f.session.config
            f.session.config.ramMiB += 128; f.session.config = original
            XCTAssertFalse(ticket.effectAdmission.isValid)
            XCTAssertNotNil(ticket.invalidationReason); XCTAssertTrue(f.session.hasPendingGUIStart)
            XCTAssertNil(f.session.reserveRuntimeMutation(kind: .snapshot))
            gate.release(); try await f.wait { !ticket.workerPending }
            guard case .refused = ticket.outcome else { throw CocoaError(.executableRuntimeMismatch) }
            XCTAssertFalse(FileManager.default.fileExists(atPath: f.base.config.evidenceDir))
            XCTAssertEqual(f.effects.snapshot.launches, 0); XCTAssertTrue(f.effects.snapshot.keyCreation.isEmpty)
            XCTAssertFalse(f.session.hasActiveRuntimeWork)
        } catch { await f.clean(); throw error }
        await f.clean()
    }

    func testAdmissionRefusalHasNoWorkerEffectsAndNoPendingTicket() async throws {
        let f = try HvfGUIStartFixture()
        f.session.workAdmission = { _ in "another library operation owns this VM" }
        let result = f.session.requestGUIStart(configuration: f.base.config)
        if case .refused = result {} else { XCTFail("Expected admission refusal") }
        XCTAssertFalse(f.session.hasPendingGUIStart)
        XCTAssertEqual(f.effects.snapshot.probes, 0); XCTAssertEqual(f.effects.snapshot.launches, 0)
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.base.config.evidenceDir))
        await f.clean()
    }
    func testOrdinaryGUIAttachmentPreservesDiagnosticAndCannotGrantOwnedControl() async throws {
        let f = try HvfGUIStartFixture(foundExisting: true)
        do {
            var nestedRefused = false
            let diagnostic = BvAgentEvent.unknown("attached to the already running HVF engine; duplicate launch prevented")
            let observer = f.session.$events.dropFirst().sink { events in
                guard events.contains(diagnostic) else { return }
                if case .refused = f.session.requestGUIStart(configuration: f.base.config) { nestedRefused = true }
            }
            defer { observer.cancel() }
            let ticket = try f.start(); try await f.wait { !ticket.workerPending }
            XCTAssertEqual(ticket.outcome, .observedAttachment); XCTAssertTrue(nestedRefused)
            XCTAssertTrue(f.session.events.contains(diagnostic)); XCTAssertTrue(f.session.hasRetainedAttachment)
            XCTAssertEqual(f.effects.snapshot.probes, 1); XCTAssertEqual(f.effects.snapshot.launches, 0)
            XCTAssertEqual(f.session.stopOwned(expectedToken: UUID()), .notOwned)
            XCTAssertFalse(FileManager.default.fileExists(atPath: f.base.config.ctlFilePath))
        } catch { await f.clean(); throw error }
        await f.clean()
    }

    func testAttachmentPublicationInvalidationNeverStopsObservedExternalRuntime() async throws {
        let f = try HvfGUIStartFixture(foundExisting: true)
        do {
            var invalidated = false
            let observer = f.session.$events.dropFirst().sink { _ in
                guard !invalidated, f.session.guiStartOperation?.workerPending == true else { return }
                invalidated = true; f.session.config.ramMiB += 128
            }
            defer { observer.cancel() }
            let ticket = try f.start(); try await f.wait { !ticket.workerPending }
            XCTAssertTrue(invalidated)
            if case .refused = ticket.outcome {} else { XCTFail("Invalidated attachment cannot publish success") }
            XCTAssertFalse(f.session.hasRetainedAttachment); XCTAssertNil(f.session.ownedProcessIdentity)
            XCTAssertEqual(f.effects.snapshot.probes, 1); XCTAssertEqual(f.effects.snapshot.launches, 0)
            XCTAssertTrue(f.effects.snapshot.keyCreation.isEmpty)
        } catch { await f.clean(); throw error }
        await f.clean()
    }

}
