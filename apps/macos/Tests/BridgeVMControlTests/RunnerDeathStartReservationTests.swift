import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class RunnerDeathStartReservationTests: XCTestCase {
    private var pipeEnds: [FileHandle] = []
    private func spawn(_ f: HvfOwnedRuntimeControllerTests.Fixture) throws -> HvfOwnedRuntimeSpawn {
        let channel = try HvfOwnedRuntimeChannel()
        // Keep synthetic peer ends open while production adoption starts its channel reactor.
        pipeEnds.append(FileHandle(fileDescriptor: dup(channel.input.fileHandleForReading.fileDescriptor), closeOnDealloc: true))
        pipeEnds.append(FileHandle(fileDescriptor: dup(channel.output.fileHandleForWriting.fileDescriptor), closeOnDealloc: true))
        return HvfOwnedRuntimeSpawn(process: f.process, channel: channel,
            guestShutdown: HvfOwnedGuestShutdown(path: f.base.config.ctlFilePath),
            token: f.controller.identity.token, config: f.base.config,
            digest: String(repeating: "a", count: 64), hello: Data())
    }

    func testOwnedStartDeathThenGUIReplacementDoesNotKeepOldReservation() async throws {
        let old = try HvfOwnedRuntimeControllerTests.Fixture(); defer { old.clean() }
        let replacement = try HvfOwnedRuntimeControllerTests.Fixture(); defer { replacement.clean() }
        let session = old.base.session()
        let firstSpawn = try spawn(old)
        session.ownedStartWorker = { _ in .success(firstSpawn) }
        let ticket = try HvfOwnedStartTestSupport.admit(session)
        try await HvfOwnedStartTestSupport.wait { !ticket.workerPending }
        let dead = try XCTUnwrap(session.ownedController)
        dead.receive(.message(try old.event("ready", sequence: 1)))
        // A successful start remains historical proof even after its VM later dies.
        dead.receive(.message(try old.event("helper-started", sequence: 2)))
        XCTAssertEqual(ticket.observation.phase, .started)
        let proof = ticket.observation.proof
        try await old.exit(); dead.tick(); dead.receive(.eof)
        XCTAssertFalse(ticket.reservesSession, "Runner death ends the reservation, not historical proof")
        XCTAssertTrue(session.mayHaveOwnedWork, "The controller still owns unconfirmed cleanup")
        let nextSpawn = try spawn(replacement)
        session.guiStartWorker = { _ in .init(result: .owned(nextSpawn)) }
        guard case let .accepted(gui) = session.requestGUIStart(configuration: session.config) else {
            return XCTFail("GUI retry must be admitted after runner death")
        }
        try await HvfOwnedStartTestSupport.wait { !gui.workerPending }
        let current = try XCTUnwrap(session.ownedController)
        current.receive(.message(try replacement.event("ready", sequence: 1)))
        current.receive(.message(try replacement.event("complete-not-admitted", sequence: 2, success: true)))
        try await replacement.exit(); current.tick(); current.receive(.eof)
        XCTAssertTrue(current.teardownConfirmed)
        XCTAssertFalse(session.hasPendingRuntimeStart, "Dead CLI ticket must not reserve the replacement session")
        XCTAssertFalse(session.hasActiveRuntimeWork)
        let observation = try XCTUnwrap(session.ownedStartObservation(operationID: ticket.operationID))
        XCTAssertFalse(ticket.reservesSession)
        XCTAssertEqual(observation.phase, .started, "CLI polling retains historical startup success")
        XCTAssertNil(observation.failure)
        XCTAssertEqual(observation.target, dead.identity)
        XCTAssertEqual(observation.proof, proof)
        _ = try NativeRuntimeStartObservation(observation) // Validate the actual polling wire contract.
        session.guiStartWorker = { _ in .init(result: .refused("synthetic subsequent start")) }
        guard case let .accepted(last) = session.requestGUIStart(configuration: session.config) else {
            return XCTFail("Start after replacement cleanup must be accepted")
        }
        try await HvfOwnedStartTestSupport.wait { !last.workerPending }
    }

    func testUnprovenExitReleasesReservationWithoutRewritingUnconfirmedObservation() async throws {
        let f = try HvfOwnedRuntimeControllerTests.Fixture(); defer { f.clean() }
        let ticket = HvfOwnedStartOperation(operationID: UUID(), configuration: f.base.config,
            expectedSavedConfigurationDigest: HvfOwnedStartTestSupport.digest, now: 100)
        ticket.finishWorker(target: f.controller.identity)
        ticket.observe(f.controller, now: 101)
        XCTAssertEqual(ticket.observation.phase, .awaitingStartup)
        XCTAssertTrue(ticket.reservesSession)
        try await f.exit()
        ticket.observe(f.controller, now: 102)
        XCTAssertFalse(ticket.reservesSession)
        XCTAssertEqual(ticket.observation.phase, .unconfirmed)
        XCTAssertEqual(ticket.observation.failure, .runnerExited)
        XCTAssertTrue(ticket.observation.mayHaveOwnedWork)
        _ = try NativeRuntimeStartObservation(ticket.observation)
    }

    func testGUIRunnerDeathAllowsOwnedStartRetryToReachLeaseCheck() async throws {
        let f = try HvfOwnedRuntimeControllerTests.Fixture(); defer { f.clean() }
        let session = f.base.session()
        let first = try spawn(f)
        session.guiStartWorker = { _ in .init(result: .owned(first)) }
        guard case let .accepted(gui) = session.requestGUIStart(configuration: session.config) else { return XCTFail("GUI start") }
        try await HvfOwnedStartTestSupport.wait { !gui.workerPending }
        let dead = try XCTUnwrap(session.ownedController)
        try await f.exit(); dead.tick(); dead.receive(.eof)
        // Missing lease executable gives a hermetic refusal before keys, preparation or a runner launch.
        guard case let .accepted(cli) = session.requestOwnedStart(configuration: session.config, operationID: UUID(),
            expectedSavedConfigurationDigest: HvfOwnedStartTestSupport.digest, onReserved: { _ in }, revalidate: { true }) else {
            return XCTFail("CLI retry must reach worker lease admission after GUI runner death")
        }
        try await HvfOwnedStartTestSupport.wait { !cli.workerPending }
        XCTAssertEqual(cli.observation.phase, .failed)
        XCTAssertEqual(cli.observation.failure, .preparation, "Retry must try media leases before runner readiness")
        XCTAssertFalse(session.hasPendingRuntimeStart)
        XCTAssertTrue(dead.mayHaveOwnedWork, "No invented cleanup proof")
    }
}
