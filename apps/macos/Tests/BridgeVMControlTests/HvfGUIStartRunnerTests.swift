import Combine
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfGUIStartRunnerTests: XCTestCase {
    func testActualGUIWorkerRunsTypedSupervisorAndCompletesOwnedStop() async throws {
        let f = try HvfGUIStartFixture(encrypted: true, actualRunner: true)
        do {
            let ticket = try f.start()
            try await f.wait({ !ticket.workerPending }, seconds: 5)
            guard case let .ownedLaunchAccepted(identity) = ticket.outcome else { throw CocoaError(.executableRuntimeMismatch) }
            try await f.wait({ try? f.observeChildren(); f.observeRuntime(); return f.childrenObserved }, seconds: 5)
            XCTAssertEqual(f.effects.snapshot.keyCreation, [true]); XCTAssertEqual(f.effects.snapshot.existingReads, 0)
            XCTAssertTrue(f.effects.snapshot.offMain.allSatisfy { $0 })
            guard case let .accepted(stop) = f.session.requestOwnedStop(target: identity, operationID: UUID()) else {
                throw CocoaError(.executableRuntimeMismatch)
            }
            try await f.wait({ f.observeRuntime(); return stop.observation.isComplete }, seconds: 5)
            XCTAssertTrue(f.childrenExited); XCTAssertFalse(f.session.mayHaveOwnedWork)
            XCTAssertEqual(stop.observation.runnerExit?.identity, identity)
            XCTAssertEqual(stop.observation.supervisorComplete?.helper.reapedCount, 1)
            XCTAssertEqual(stop.observation.supervisorComplete?.swtpm.reapedCount, 1)
        } catch { await f.clean(); throw error }
        await f.clean()
    }

    func testLateTypedProcessResultRetainsAndCancelsExactRunnerAfterInvalidation() async throws {
        let gate = HvfGUIStartGate()
        let f = try HvfGUIStartFixture(encrypted: true, actualRunner: true, launchGate: gate)
        do {
            let ticket = try f.start(); try await f.wait { gate.state.entered }
            let child = try XCTUnwrap(f.effects.snapshot.child)
            XCTAssertTrue(child.isRunning); XCTAssertTrue(ticket.workerPending)
            let replacement = f.root.appendingPathComponent("unrelated-control")
            try Data("leave unchanged\n".utf8).write(to: replacement)
            f.session.config.ctlFilePath = replacement.path
            XCTAssertTrue(f.session.hasActiveRuntimeWork); XCTAssertNil(f.session.reserveRuntimeMutation(kind: .snapshot))
            gate.release(); try await f.wait { !ticket.workerPending }
            if case .refused = ticket.outcome {} else { XCTFail("Stale spawned result cannot report start acceptance") }
            let controller = try XCTUnwrap(f.session.ownedController)
            XCTAssertTrue(controller.process === child); XCTAssertEqual(controller.identity.processID, child.processIdentifier)
            XCTAssertNotNil(controller.operation)
            try await f.wait({ try? f.observeChildren(); return controller.teardownConfirmed }, seconds: 5)
            XCTAssertFalse(child.isRunning); XCTAssertNotNil(controller.ledger.complete); XCTAssertNotNil(controller.runnerExit)
            XCTAssertEqual(try String(contentsOf: replacement, encoding: .utf8), "leave unchanged\n")
            XCTAssertFalse(FileManager.default.fileExists(atPath: f.root.appendingPathComponent("guest-command-observed").path))
        } catch { await f.clean(); throw error }
        await f.clean()
    }

    func testPublicationInvalidationCannotActivateGuestInputOrReportStartSuccess() async throws {
        let f = try HvfGUIStartFixture(encrypted: true, actualRunner: true)
        do {
            let replacement = f.root.appendingPathComponent("publication-control")
            try Data("leave unchanged\n".utf8).write(to: replacement)
            var invalidated = false
            let observer = f.session.$events.dropFirst().sink { _ in
                guard !invalidated, f.session.guiStartOperation?.workerPending == true else { return }
                invalidated = true; f.session.config.ctlFilePath = replacement.path
            }
            defer { observer.cancel() }
            let ticket = try f.start(); try await f.wait({ !ticket.workerPending }, seconds: 5)
            XCTAssertTrue(invalidated)
            if case .refused = ticket.outcome {} else { XCTFail("Publication invalidation must refuse the now-stale result") }
            let controller = try XCTUnwrap(f.session.ownedController)
            try await f.wait({ try? f.observeChildren(); return controller.teardownConfirmed }, seconds: 5)
            XCTAssertEqual(try String(contentsOf: replacement, encoding: .utf8), "leave unchanged\n")
            XCTAssertFalse(FileManager.default.fileExists(atPath: f.root.appendingPathComponent("guest-command-observed").path))
        } catch { await f.clean(); throw error }
        await f.clean()
    }

    func testLateLegacyProcessAndKeyDeliveryFailureStayOwnedUntilObservedExit() async throws {
        for deliveryFailure in [false, true] {
            let gate = deliveryFailure ? nil : HvfGUIStartGate()
            let f = try HvfGUIStartFixture(encrypted: true, failKeyDelivery: deliveryFailure, launchGate: gate)
            do {
                let ticket = try f.start()
                let unrelated = f.root.appendingPathComponent("unrelated-legacy")
                let control = unrelated.appendingPathComponent("control")
                if let gate {
                    try await f.wait { gate.state.entered }
                    try FileManager.default.createDirectory(at: unrelated, withIntermediateDirectories: true)
                    try Data("BVAGENT SERVICE start t=1\nBVAGENT READY host=unrelated t=2\n".utf8).write(to: unrelated.appendingPathComponent("run.log"))
                    try Data("leave unchanged\n".utf8).write(to: control)
                    var changed = f.session.config
                    changed.evidenceDir = unrelated.path; changed.ctlFilePath = control.path
                    f.session.config = changed; gate.release()
                }
                try await f.wait { !ticket.workerPending }
                if deliveryFailure {
                    if case .failed(.keyDelivery, _) = ticket.outcome {} else { XCTFail("Expected delivery failure") }
                    XCTAssertTrue(f.effects.snapshot.deliveryPipeClosed)
                } else if case .refused = ticket.outcome {} else { XCTFail("Expected invalidated late result") }
                let child = try XCTUnwrap(f.effects.snapshot.child)
                XCTAssertTrue(FileManager.default.fileExists(atPath: f.root.appendingPathComponent("shell-ready").path))
                XCTAssertTrue(child.isRunning, "Fixture acknowledged TERM-ignoring handler before returning")
                XCTAssertTrue(f.session.process === child); XCTAssertTrue(f.session.hasActiveRuntimeWork)
                XCTAssertNil(f.session.reserveRuntimeMutation(kind: .snapshot))
                XCTAssertEqual(f.effects.snapshot.keyCreation, [true])
                if !deliveryFailure {
                    XCTAssertFalse(f.session.serviceStarted, "Stale cleanup must not poll the replacement evidence path")
                    XCTAssertEqual(try String(contentsOf: control, encoding: .utf8), "leave unchanged\n")
                }
                try Data().write(to: f.root.appendingPathComponent("fixture-release"))
                try await f.wait({ return f.session.lastOwnedExit != nil }, seconds: 3)
                XCTAssertEqual(f.session.lastOwnedExit?.status, 17); XCTAssertFalse(f.session.hasActiveRuntimeWork)
            } catch { await f.clean(); throw error }
            await f.clean()
        }
    }
}
