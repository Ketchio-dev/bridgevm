import XCTest
@testable import BridgeVMControl

@MainActor
final class ReservedCloneLifetimeTests: XCTestCase {
    func testAcceptedSourceSurvivesChangedRegistration() async throws {
        try await checkReload(removesSource: false)
    }

    func testAcceptedSourceSurvivesRemovedRegistration() async throws {
        try await checkReload(removesSource: true)
    }

    private func checkReload(removesSource: Bool) async throws {
        let f = try ReservedCloneGenericFixture(kind: "fast-vz", shared: true)
        defer { f.clean() }
        f.admitClone()
        let registration = f.assets.root.appendingPathComponent(f.selected.slug + "/vm.json")
        if removesSource { try FileManager.default.removeItem(at: registration) }
        else {
            var changed = try f.assets.config("replacement", kind: "hvf-engine")
            changed.id = f.selected.id
            f.assets.save(changed)
        }
        let savedAfterEdit = try? Data(contentsOf: registration)
        f.library.reload()
        XCTAssertFalse(f.library.vms.contains(f.selected))
        XCTAssertTrue(f.library.cloningSlugs.contains(f.selected.slug))
        f.model.start()
        await f.assets.wait { !f.model.hasAcceptedOperation }
        XCTAssertEqual(f.backend.calls[0], 0)
        XCTAssertNotNil(f.library.operationError)
        try f.assertUnchanged(ignoringSelectedRegistration: true)
        XCTAssertEqual(try? Data(contentsOf: registration), savedAfterEdit)
    }

    func testQueuedDuplicateCloneAndStartsRemainSynchronous() async throws {
        let f = try ReservedCloneGenericFixture(kind: "fast-vz", shared: true)
        defer { f.clean() }
        f.admitClone()
        // No suspension between accepted reservation and the next MainActor admissions.
        f.library.cloneWindowsHVF(f.selected, name: "second")
        f.model.start()
        f.model.start()
        f.model.applyResources()
        await f.assets.wait { !f.model.hasAcceptedOperation }
        XCTAssertEqual(f.backend.calls[0], 0)
        XCTAssertFalse(f.backend.didBeginResourceUpdate)
        XCTAssertNotNil(f.library.cloneError)
        XCTAssertNotNil(f.library.operationError)
        try f.assertUnchanged()
    }

    func testUnownedStopStillWorksWhileSharedCloneIsReserved() async throws {
        let f = try ReservedCloneGenericFixture(kind: "fast-vz", shared: true)
        defer { f.clean() }
        f.admitClone()
        f.backend.setExternalRunning(true)
        f.model.running = true
        f.model.stop()
        await f.assets.wait { !f.model.hasAcceptedOperation }
        XCTAssertEqual(f.backend.calls[3], 1)
        XCTAssertEqual(f.backend.calls[0], 0)
        XCTAssertNil(f.library.operationError)
        try f.assertUnchanged()
    }

    func testRunningTransferKeepsReservationUntilCompletion() async throws {
        let f = try ReservedCloneGenericFixture(kind: "fast-vz", shared: true)
        defer { f.clean() }
        let gate = ReservedCloneTransferGate()
        f.admitClone { _, _, _ in gate.enter(); return nil }
        let job = try f.runClone()
        await f.assets.wait { gate.didEnter }
        f.model.start()
        XCTAssertTrue(f.library.cloningSlugs.contains(f.selected.slug))
        XCTAssertFalse(f.model.hasAcceptedOperation)
        XCTAssertEqual(f.backend.calls[0], 0)
        XCTAssertNotNil(f.library.operationError)
        gate.release()
        await job.value
        XCTAssertTrue(f.library.cloningSlugs.isEmpty)
        f.library.operationError = nil
        f.model.start()
        await f.assets.wait { !f.model.hasAcceptedOperation }
        XCTAssertEqual(f.backend.calls[0], 1)
        XCTAssertNil(f.library.operationError)
        XCTAssertEqual(try Data(contentsOf: f.disk), f.original)
    }

    func testFailedScheduledTransferReleasesReservation() async throws {
        try await checkCompletion(succeeds: false)
    }

    func testSuccessfulScheduledTransferReleasesReservation() async throws {
        try await checkCompletion(succeeds: true)
    }

    private func checkCompletion(succeeds: Bool) async throws {
        let f = try ReservedCloneGenericFixture(kind: "fast-vz", shared: true)
        defer { f.clean() }
        f.admitClone { name, source, root in
            guard succeeds else { return nil }
            // Fake transfer publishes only an owned synthetic registration, never media or keys.
            var result = source
            result.id = "copy"; result.name = name; result.displayName = name
            result.bundlePath = root.appendingPathComponent("copy/bundle").path
            result.diskPath = nil
            try? FileManager.default.createDirectory(atPath: result.bundlePath, withIntermediateDirectories: true)
            return VMLibrary.createOutcome(result, rootURL: root).isCommitted ? result : nil
        }
        f.model.start()
        XCTAssertNotNil(f.library.operationError)
        XCTAssertFalse(f.model.hasAcceptedOperation)
        XCTAssertEqual(f.backend.calls[0], 0)
        await Task.yield()
        XCTAssertTrue(f.library.cloningSlugs.contains(f.selected.slug))
        try await f.finishClone()
        XCTAssertEqual(f.library.cloneError == nil, succeeds)
        XCTAssertEqual(f.library.vms.contains { $0.slug == "copy" }, succeeds)
        if succeeds { XCTAssertEqual(f.library.selectedID, "copy") }
        f.library.operationError = nil
        f.model.start()
        await f.assets.wait { !f.model.hasAcceptedOperation }
        XCTAssertEqual(f.backend.calls[0], 1)
        XCTAssertNil(f.library.operationError)
        XCTAssertEqual(try Data(contentsOf: f.disk), f.original)
        for (url, bytes) in f.registrations { XCTAssertEqual(try Data(contentsOf: url), bytes) }
    }
}

private final class ReservedCloneTransferGate: @unchecked Sendable {
    private let lock = NSLock()
    private let gate = DispatchSemaphore(value: 0)
    private var entered = false
    var didEnter: Bool { lock.withLock { entered } }
    func enter() {
        lock.withLock { entered = true }
        XCTAssertEqual(gate.wait(timeout: .now() + 5), .success)
    }
    func release() { gate.signal() }
}
