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

    private func wait(_ predicate: @MainActor () -> Bool) async throws {
        let deadline = ProcessInfo.processInfo.systemUptime + 2
        while !predicate(), ProcessInfo.processInfo.systemUptime < deadline { try await Task.sleep(nanoseconds: 5_000_000) }
        XCTAssertTrue(predicate(), "Bounded asynchronous fixture did not finish")
    }

    /// A CLI start after the runner died is the Start button's retry: it reaches the media-lease
    /// check (and says so in one sentence) before keys, preparation or a runner launch.
    func testRunnerDeathAllowsCLIStartRetryToReachLeaseCheck() async throws {
        let f = try HvfOwnedRuntimeControllerTests.Fixture(); defer { f.clean() }
        let session = f.base.session()
        let first = try spawn(f)
        session.guiStartWorker = { _ in .init(result: .owned(first)) }
        guard case let .accepted(gui) = session.requestGUIStart(configuration: session.config) else { return XCTFail("GUI start") }
        try await wait { !gui.workerPending }
        let dead = try XCTUnwrap(session.ownedController)
        try await f.exit(); dead.tick(); dead.receive(.eof)
        // Missing lease executable gives a hermetic refusal before keys, preparation or a runner launch.
        session.guiStartWorker = HvfGUIStartWorker.run
        guard case let .accepted(cli) = session.requestGUIStart(configuration: session.config, policy: .requireNew) else {
            return XCTFail("CLI retry must reach worker lease admission after runner death")
        }
        try await wait { !cli.workerPending }
        guard case let .refused(reason) = cli.outcome else { return XCTFail("retry outcome: \(String(describing: cli.outcome))") }
        XCTAssertTrue(reason.contains("정리"), "Retry must try media leases before runner readiness: \(reason)")
        XCTAssertFalse(session.hasPendingRuntimeStart)
        XCTAssertTrue(dead.mayHaveOwnedWork, "No invented cleanup proof")
    }
}
