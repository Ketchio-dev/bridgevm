import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryStartupCoordinatorTests: XCTestCase {
    func testRecoveryRunsOffMainActorAndCoalescesCancelledUIWaiters() async throws {
        let fixture = try LibraryStartupFixture(), probe = LibraryStartupProbe()
        defer { fixture.clean() }
        try fixture.recovery.stageInterruption()
        let startup = fixture.coordinator(probe: probe)
        let first = Task { await startup.load() }
        let entered = await probe.waitForEntry()
        XCTAssertTrue(entered)
        let second = Task { await startup.load() }
        var acknowledged = false
        await Task { @MainActor in acknowledged = true }.value
        XCTAssertTrue(acknowledged)
        XCTAssertNil(startup.model); XCTAssertNil(fixture.cache.retainedValue)
        XCTAssertTrue(FileManager.default.fileExists(atPath: fixture.recovery.paths.journal.path))
        first.cancel()
        probe.release()
        await first.value; await second.value
        let model = try XCTUnwrap(startup.model)
        XCTAssertTrue(model === fixture.cache.retainedValue)
        XCTAssertEqual(fixture.publications, 1)
        XCTAssertEqual(probe.observations.threads, [false]); XCTAssertFalse(probe.observations.timedOut)
        XCTAssertNil(startup.failure); XCTAssertTrue(model.libraryIssues.isEmpty)
        XCTAssertEqual(model.vms.first?.installPending, false)
        XCTAssertEqual(try Data(contentsOf: fixture.recovery.paths.finalDisk), fixture.recovery.disk)
        XCTAssertEqual(try Data(contentsOf: fixture.recovery.paths.finalVars), fixture.recovery.vars)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.recovery.paths.journal.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.recovery.paths.pendingRequest.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: fixture.recovery.paths.doneRequest.path))
        await startup.load()
        XCTAssertEqual(fixture.publications, 1)
    }

    func testClosingDuringScanKeepsLeaseUntilAcknowledgementAndNeverPublishes() async throws {
        let fixture = try LibraryStartupFixture(), probe = LibraryStartupProbe()
        defer { fixture.clean() }
        try fixture.recovery.stageInterruption()
        let startup = fixture.coordinator(probe: probe)
        let waiter = Task { await startup.load() }
        let entered = await probe.waitForEntry()
        XCTAssertTrue(entered)
        startup.close(); startup.close(); waiter.cancel()
        XCTAssertEqual(fixture.closes, 0)
        XCTAssertThrowsError(try NativeRuntimeOwner(library: fixture.library)) {
            XCTAssertEqual($0 as? NativeRuntimeError, .ownerBusy)
        }
        await startup.load()
        probe.release(); await waiter.value
        XCTAssertNil(startup.model); XCTAssertNil(fixture.cache.retainedValue)
        XCTAssertEqual(fixture.publications, 0); XCTAssertEqual(fixture.closes, 1)
        XCTAssertEqual(probe.observations.threads, [false]); XCTAssertFalse(probe.observations.timedOut)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.recovery.paths.journal.path))
        let replacement = try NativeRuntimeOwner(library: fixture.library)
        replacement.close()
        startup.close()
        XCTAssertEqual(fixture.closes, 1)
    }

    func testReplacedLibraryCannotPublishStaleScanOrStartAnotherWorker() async throws {
        let fixture = try LibraryStartupFixture(), probe = LibraryStartupProbe()
        defer { fixture.clean() }
        let startup = fixture.coordinator(probe: probe)
        let waiter = Task { await startup.load() }
        let entered = await probe.waitForEntry()
        XCTAssertTrue(entered)
        try FileManager.default.moveItem(at: fixture.recovery.library,
            to: fixture.recovery.root.appendingPathComponent("original-library"))
        try FileManager.default.createDirectory(at: fixture.recovery.library, withIntermediateDirectories: true)
        probe.release(); await waiter.value
        XCTAssertNil(startup.model); XCTAssertNil(fixture.cache.retainedValue)
        XCTAssertNotNil(startup.failure); XCTAssertEqual(fixture.publications, 0)
        await startup.load()
        XCTAssertEqual(probe.observations.threads, [false]); XCTAssertFalse(probe.observations.timedOut)
        startup.close()
    }

    func testFailedWorkerCanRetryAndClosingBeforeLoadDoesNotScan() async throws {
        let fixture = try LibraryStartupFixture(), probe = LibraryStartupProbe(failFirst: true)
        defer { fixture.clean() }
        let startup = fixture.coordinator(probe: probe)
        let waiter = Task { await startup.load() }
        let entered = await probe.waitForEntry()
        XCTAssertTrue(entered)
        probe.release(); await waiter.value
        XCTAssertNotNil(startup.failure); XCTAssertNil(startup.model)
        XCTAssertEqual(fixture.publications, 0)
        await startup.load()
        XCTAssertNil(startup.failure); XCTAssertNotNil(startup.model)
        XCTAssertEqual(fixture.publications, 1); XCTAssertEqual(probe.observations.threads, [false, false])
        let closed = fixture.coordinator(probe: probe)
        closed.close(); await closed.load()
        XCTAssertNil(closed.model); XCTAssertEqual(fixture.closes, 1)
        XCTAssertEqual(probe.observations.threads, [false, false])
    }
}
