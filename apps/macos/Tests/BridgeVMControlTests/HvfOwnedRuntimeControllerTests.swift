import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfOwnedRuntimeControllerTests: XCTestCase {
    final class Clock { var value: TimeInterval = 100 }
    /// Only the retained shell is real. Protocol events are synthetic fault injections.
    @MainActor final class Fixture {
        let base: HvfOwnedRuntimeFixture
        let process = Process(), gate = Pipe(), clock = Clock()
        let channel: HvfOwnedRuntimeChannel
        let controller: HvfOwnedRunController
        init(guestQueue: DispatchQueue = DispatchQueue(label: "owned-controller-test-guest")) throws {
            base = try HvfOwnedRuntimeFixture()
            try Data().write(to: URL(fileURLWithPath: base.config.ctlFilePath))
            channel = try HvfOwnedRuntimeChannel()
            process.executableURL = URL(fileURLWithPath: "/bin/sh")
            process.arguments = ["-c", "read value; exit 7"]
            process.standardInput = gate; process.standardOutput = FileHandle.nullDevice
            process.standardError = FileHandle.nullDevice
            try process.run()
            let clock = clock
            controller = HvfOwnedRunController(identity: .init(token: UUID(), processID: process.processIdentifier),
                config: base.config, process: process, manifestSHA256: String(repeating: "a", count: 64),
                channel: channel, guestShutdown: HvfOwnedGuestShutdown(path: base.config.ctlFilePath, queue: guestQueue),
                now: { clock.value })
        }
        func event(_ name: String, sequence: UInt64, success: Bool = false) throws -> HvfOwnedRuntimeEvent {
            var object = try XCTUnwrap(JSONSerialization.jsonObject(with: HvfOwnedRuntimeCodecTests.data(name)) as? [String: Any])
            object["runnerPID"] = process.processIdentifier
            object["runToken"] = controller.identity.token.uuidString.lowercased()
            object["sequence"] = sequence
            if success {
                var complete = try XCTUnwrap(object["complete"] as? [String: Any])
                complete["cause"] = "normalExit"; complete["outcome"] = "finished"
                complete["failureCode"] = NSNull(); complete["mediaLeaseDisposition"] = "releasedAfterReap"
                object["complete"] = complete
            }
            return try JSONDecoder().decode(HvfOwnedRuntimeEvent.self,
                from: JSONSerialization.data(withJSONObject: object, options: [.sortedKeys, .withoutEscapingSlashes]))
        }
        /// Runner bytes as a newer runner might write them: other key order, omitted nulls and an extra field.
        func decoded(_ kind: String, sequence: Int, _ body: String) throws -> HvfOwnedRuntimeEvent {
            try HvfOwnedRuntimeCodec.decode(HvfOwnedRuntimeEvent.self, payload: Data(("{\"sequence\":\(sequence),"
                + "\"kind\":\"\(kind)\",\"runnerPID\":\(process.processIdentifier),\"addedLater\":{\"x\":[1,2]},"
                + "\"runToken\":\"\(controller.identity.token.uuidString.lowercased())\",\"schemaVersion\":1,\(body)}").utf8))
        }
        func exit() async throws {
            try gate.fileHandleForWriting.close()
            let deadline = ProcessInfo.processInfo.systemUptime + 2
            while process.isRunning, ProcessInfo.processInfo.systemUptime < deadline {
                try await Task.sleep(nanoseconds: 5_000_000)
            }
            XCTAssertFalse(process.isRunning)
            controller.tick()
        }
        func clean() {
            try? gate.fileHandleForWriting.close(); process.waitUntilExit()
            controller.closeOwnerChannel(); base.clean()
        }
        func stop() throws -> HvfOwnedStopOperation {
            guard case let .accepted(ticket) = controller.requestStop(target: controller.identity, operationID: UUID()) else {
                throw CocoaError(.executableRuntimeMismatch)
            }
            return ticket
        }
    }

    func testReceiptAndExitRemainPendingUntilTerminalStreamIsValidated() async throws {
        let f = try Fixture(); defer { f.clean() }
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        let ticket = try f.stop()
        f.controller.receive(.message(try f.event("complete-not-admitted", sequence: 2, success: true)))
        XCTAssertEqual(ticket.observation.phase, .awaitingRunnerExit)
        XCTAssertTrue(f.controller.mayHaveOwnedWork)
        try await f.exit()
        XCTAssertTrue(f.controller.mayHaveOwnedWork, "Exited runner may still have buffered protocol bytes")
        XCTAssertFalse(ticket.observation.isComplete)
        f.controller.receive(.eof)
        XCTAssertTrue(ticket.observation.isComplete)
        XCTAssertFalse(f.controller.mayHaveOwnedWork)
        XCTAssertEqual(ticket.observation.runnerExit?.status, 7, "Cleanup proof does not mean runner exit zero")
    }

    /// A newer runner may add fields, order keys differently or change its timeouts; that used to close
    /// the channel as a protocol error and leave the Stop unconfirmed until the user forced it.
    func testRunnerEventsWithUnknownFieldsKeyOrderAndOtherTimeoutsStillCompleteTheStop() async throws {
        let f = try Fixture(); defer { f.clean() }
        f.controller.receive(.message(try f.decoded("ready", sequence: 1, "\"ready\":{\"termMillis\":8000,"
            + "\"killReapMillis\":5000,\"generation\":0,\"manifestSHA256\":\"\(String(repeating: "a", count: 64))\",\"note\":1}")))
        let id = UUID(), op = id.uuidString.lowercased()
        guard case let .accepted(ticket) = f.controller.requestStop(target: f.controller.identity, operationID: id,
                                                                     allowGuestGrace: false) else { return XCTFail("stop refused") }
        f.controller.receive(.message(try f.decoded("stopAck", sequence: 2,
            "\"stopAck\":{\"disposition\":\"accepted\",\"operationID\":\"\(op)\"}")))
        let none = "{\"reapedCount\":0,\"spawnedCount\":0}"
        f.controller.receive(.message(try f.decoded("complete", sequence: 3, "\"complete\":{\"outcome\":\"cancelled\","
            + "\"cause\":\"stopRequested\",\"operationID\":\"\(op)\",\"helper\":\(none),\"swtpm\":\(none),"
            + "\"mediaLeaseDisposition\":\"releasedAfterReap\",\"runtimeDirectoryDisposition\":\"removed\"}")))
        try await f.exit()
        f.controller.receive(.eof)
        XCTAssertTrue(ticket.observation.isComplete)
        XCTAssertNil(ticket.observation.failure)
        XCTAssertFalse(f.controller.mayHaveOwnedWork)
    }

    /// A cause or failure code this app does not know is still a runner failure, reported as such.
    func testUnknownCompleteStringsAreACleanupFailureNotAProtocolError() async throws {
        let f = try Fixture(); defer { f.clean() }
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        let ticket = try f.stop()
        let none = "{\"last\":null,\"reapedCount\":0,\"spawnedCount\":0}"
        f.controller.receive(.message(try f.decoded("complete", sequence: 2, "\"complete\":{\"outcome\":\"failed\","
            + "\"cause\":\"hostSleep\",\"failureCode\":\"diskFull\",\"helper\":\(none),\"swtpm\":\(none),"
            + "\"mediaLeaseDisposition\":\"releasedAfterReap\",\"runtimeDirectoryDisposition\":\"keptForDebugging\"}")))
        try await f.exit()
        f.controller.receive(.eof)
        XCTAssertEqual(ticket.observation.phase, .unconfirmed)
        XCTAssertEqual(ticket.observation.failure, .cleanupFailed)
        XCTAssertFalse(f.controller.supervisionLost)
        XCTAssertFalse(f.controller.mayHaveOwnedWork, "a valid complete and exit still release ownership")
    }

    func testDuplicateCompleteAfterRunnerExitInvalidatesProofAndKeepsOwnership() async throws {
        let f = try Fixture(); defer { f.clean() }
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        let ticket = try f.stop()
        let complete = try f.event("complete-not-admitted", sequence: 2, success: true)
        f.controller.receive(.message(complete)); try await f.exit()
        f.controller.receive(.message(complete)); f.controller.receive(.eof)
        XCTAssertFalse(ticket.observation.isComplete)
        XCTAssertEqual(ticket.observation.failure, .protocolInvalid)
        XCTAssertTrue(f.controller.mayHaveOwnedWork)
    }

    func testAlreadyClosedControlInputStillAllowsQueuedSpontaneousCompletion() async throws {
        let f = try Fixture(); defer { f.clean() }
        f.channel.close()
        let ticket = try f.stop()
        XCTAssertNil(ticket.observation.failure)
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        f.controller.receive(.message(try f.event("complete-not-admitted", sequence: 2, success: true)))
        try await f.exit(); f.controller.receive(.eof)
        XCTAssertTrue(ticket.observation.isComplete)
        XCTAssertFalse(f.controller.mayHaveOwnedWork)
    }

    func testPreReadyCancellationIsBoundedAndRepeatedStopReturnsSameTicket() throws {
        let f = try Fixture(); defer { f.clean() }
        let first = try f.stop()
        guard case let .existing(second) = f.controller.requestStop(target: f.controller.identity, operationID: UUID()) else {
            return XCTFail("Expected durable admission")
        }
        XCTAssertTrue(first === second)
        XCTAssertEqual(first.observation.phase, .cancelling)
        XCTAssertEqual(first.observation.deadlineUptime, 112)
        XCTAssertEqual(f.controller.ledger.stopOperationID, first.operationID)
        let wrong = HvfOwnedRuntimeIdentity(token: UUID(), processID: f.process.processIdentifier)
        guard case .refused(.targetMismatch) = f.controller.requestStop(target: wrong, operationID: first.operationID) else {
            return XCTFail("Wrong target must not control the retained run")
        }
    }

    func testExitWithoutCompleteNeverProducesStoppedProof() async throws {
        let f = try Fixture(); defer { f.clean() }
        let ticket = try f.stop()
        try await f.exit(); f.controller.receive(.eof)
        XCTAssertEqual(ticket.observation.failure, .runnerExitedWithoutComplete)
        XCTAssertTrue(f.controller.mayHaveOwnedWork)
        XCTAssertNil(ticket.observation.supervisorComplete)
    }

    func testQueuedGuestEffectMustDrainBeforeOwnershipCanBeReleased() async throws {
        let queue = DispatchQueue(label: "owned-controller-delayed-guest")
        queue.suspend()
        var resumed = false
        defer { if !resumed { queue.resume() } }
        let f = try Fixture(guestQueue: queue); defer { f.clean() }
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        let ticket = try f.stop()
        f.controller.guestServiceReady()
        f.controller.receive(.message(try f.event("complete-not-admitted", sequence: 2, success: true)))
        try await f.exit(); f.controller.receive(.eof)
        XCTAssertTrue(f.controller.mayHaveOwnedWork)
        XCTAssertFalse(ticket.observation.isComplete)
        queue.resume(); resumed = true
        let deadline = ProcessInfo.processInfo.systemUptime + 2
        while f.controller.mayHaveOwnedWork, ProcessInfo.processInfo.systemUptime < deadline {
            try await Task.sleep(nanoseconds: 5_000_000)
        }
        XCTAssertFalse(f.controller.mayHaveOwnedWork)
        XCTAssertTrue(ticket.observation.isComplete)
        XCTAssertTrue(try Data(contentsOf: URL(fileURLWithPath: f.base.config.ctlFilePath)).isEmpty,
                      "Terminal cleanup closes queued guest effects before they can write")
    }

    func testFirstFinalProofAfterDeadlineCannotBypassFailureWithoutAnEarlierTick() async throws {
        for lateEOF in [true, false] {
            let f = try Fixture(); defer { f.clean() }
            f.controller.receive(.message(try f.event("ready", sequence: 1)))
            let ticket = try f.stop()
            f.controller.receive(.message(try f.event("complete-not-admitted", sequence: 2, success: true)))
            if lateEOF { try await f.exit() } else { f.controller.receive(.eof) }
            f.clock.value = 293 // Deliberately no intervening tick at the deadline.
            if lateEOF { f.controller.receive(.eof) } else { try await f.exit() }
            XCTAssertFalse(f.controller.mayHaveOwnedWork)
            XCTAssertEqual(ticket.observation.failure, .deadlineExceeded)
            XCTAssertFalse(ticket.observation.isComplete)
        }
    }

    func testGuestEffectCallbackFirstObservedAfterDeadlineKeepsFailure() async throws {
        let queue = DispatchQueue(label: "owned-controller-late-guest")
        queue.suspend(); var resumed = false
        defer { if !resumed { queue.resume() } }
        let f = try Fixture(guestQueue: queue); defer { f.clean() }
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        let ticket = try f.stop(); f.controller.guestServiceReady()
        f.controller.receive(.message(try f.event("complete-not-admitted", sequence: 2, success: true)))
        try await f.exit(); f.controller.receive(.eof)
        XCTAssertTrue(f.controller.mayHaveOwnedWork)
        f.clock.value = 293
        queue.resume(); resumed = true
        let deadline = ProcessInfo.processInfo.systemUptime + 2
        while f.controller.mayHaveOwnedWork, ProcessInfo.processInfo.systemUptime < deadline {
            try await Task.sleep(nanoseconds: 5_000_000)
        }
        XCTAssertFalse(f.controller.mayHaveOwnedWork)
        XCTAssertEqual(ticket.observation.failure, .deadlineExceeded)
        XCTAssertFalse(ticket.observation.isComplete)
    }

    func testMissedDeadlineRemainsFailedAfterLateValidCleanup() async throws {
        let f = try Fixture(); defer { f.clean() }
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        let ticket = try f.stop()
        XCTAssertEqual(ticket.observation.deadlineUptime, 292)
        f.clock.value = 280; f.controller.tick()
        XCTAssertEqual(ticket.observation.phase, .cancelling)
        f.clock.value = 292; f.controller.tick()
        XCTAssertEqual(ticket.observation.failure, .deadlineExceeded)
        f.controller.receive(.message(try f.event("complete-not-admitted", sequence: 2, success: true)))
        try await f.exit(); f.controller.receive(.eof)
        XCTAssertFalse(f.controller.mayHaveOwnedWork)
        XCTAssertEqual(ticket.observation.failure, .deadlineExceeded)
        XCTAssertFalse(ticket.observation.isComplete)
    }
}
