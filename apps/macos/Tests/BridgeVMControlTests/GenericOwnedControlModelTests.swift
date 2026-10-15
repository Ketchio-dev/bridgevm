import XCTest
@testable import BridgeVMControl

@MainActor
final class GenericOwnedControlModelTests: XCTestCase {
    func testOwnedRefusalNeverFallsBackToLegacyLaunch() async throws {
        let f = try GenericOwnedControlFixture(); defer { f.clean() }
        let backend = GenericOwnedControlBackend(); backend.plan = nil
        let model = ControlModel(config: try f.config("refused"), backend: backend, startsAutomatically: false)
        model.start()
        await f.wait { !model.lifecycleBusy }
        XCTAssertFalse(model.running)
        XCTAssertNil(model.ownedRuntime)
        XCTAssertEqual(backend.calls[0], 1)
        XCTAssertEqual(backend.calls[2], 0)
    }

    func testOwnedStopAndRefreshNeverAdoptObservedExternalReplacement() async throws {
        let f = try GenericOwnedControlFixture(); defer { f.clean() }
        let config = try f.config("owned"); f.save(config)
        let library = f.library(), model = library.model(for: config)
        model.start()
        await f.wait { model.ownedRuntime?.snapshot.primaryRunning == true && !model.lifecycleBusy }
        let owner = try XCTUnwrap(model.ownedRuntime), backend = try XCTUnwrap(f.backends.first)
        model.refreshStatus()
        await f.wait { model.statusNote == "실행 중" }
        backend.setExternalRunning(false)
        XCTAssertNotNil(library.libraryActionRefusal(for: config), "Owned activity protects files independently of path lookup")
        backend.setExternalRunning(true)
        model.stop()
        await f.wait { !model.lifecycleBusy && !owner.snapshot.isActive }
        let probes = backend.calls[1]
        model.refreshStatus()
        await f.wait { model.statusNote == "정지됨" }
        XCTAssertFalse(model.running)
        XCTAssertTrue(model.ownedRuntime === owner)
        XCTAssertEqual(backend.calls[1], probes)
        XCTAssertEqual(backend.calls[2], 0)
        XCTAssertEqual(backend.calls[3], 0)
    }

    func testResourceRestartReplacesOnlyOwnedHandle() async throws {
        let f = try GenericOwnedControlFixture(); defer { f.clean() }
        let backend = GenericOwnedControlBackend()
        let model = ControlModel(config: try f.config("resources"), backend: backend, startsAutomatically: false)
        f.models.append(model)
        model.start()
        await f.wait { model.ownedRuntime?.snapshot.primaryRunning == true && !model.lifecycleBusy }
        let old = try XCTUnwrap(model.ownedRuntime)
        model.pendingMemGiB = 8; model.pendingCPU = 4
        model.applyResources()
        await f.wait { !model.lifecycleBusy && model.ownedRuntime !== old }
        await f.wait { model.ownedRuntime?.snapshot.primaryRunning == true }
        XCTAssertFalse(old.snapshot.isActive)
        XCTAssertEqual(model.config.memMiB, 8192)
        XCTAssertEqual(backend.calls[0], 2)
        XCTAssertEqual(backend.calls[1], 0)
        XCTAssertEqual(backend.calls[2], 0)
        XCTAssertEqual(backend.calls[3], 0)
        model.stop()
        await f.wait { !model.lifecycleBusy && !model.hasActiveRuntimeWork }
    }

    func testCurrentRegisteredUnownedStopKeepsLegacyCompatibility() async throws {
        let f = try GenericOwnedControlFixture(); defer { f.clean() }
        let config = try f.config("external"); f.save(config)
        let library = f.library(), model = library.model(for: config)
        model.running = true
        model.stop()
        await f.wait { !model.lifecycleBusy }
        XCTAssertEqual(f.backends[0].calls[3], 1)
        XCTAssertNil(model.ownedRuntime)
        XCTAssertFalse(model.running)
    }
    func testAcceptedResourceRestartCompletesOnRetainedOriginalModel() async throws {
        let f = try GenericOwnedControlFixture(); defer { f.clean() }
        let config = try f.config("resources"), other = try f.config("other")
        f.save(config); f.save(other)
        let library = f.library(), model = library.model(for: config)
        model.start()
        await f.wait { !model.lifecycleBusy && model.ownedRuntime?.snapshot.primaryRunning == true }
        model.refreshStatus()
        await f.wait { model.statusNote == "실행 중" && !model.hasAcceptedOperation }
        let old = try XCTUnwrap(model.ownedRuntime), backend = try XCTUnwrap(f.backends.first)
        let gate = DispatchSemaphore(value: 0)
        backend.resourceGate = gate
        defer { gate.signal() }
        model.pendingMemGiB = 8; model.pendingCPU = 4
        model.applyResources()
        await f.wait { backend.didBeginResourceUpdate }
        try FileManager.default.removeItem(at: f.root.appendingPathComponent("resources/vm.json"))
        library.selectedID = other.slug
        library.reload()
        let record = try XCTUnwrap(library.retainedControlRecords.first)
        XCTAssertTrue(model.isRetained)
        XCTAssertTrue(record.descriptor.isActive)
        XCTAssertFalse(library.dismissRetainedControl(record.id))
        gate.signal()
        await f.wait { !model.lifecycleBusy && model.ownedRuntime !== old }
        await f.wait { model.ownedRuntime?.snapshot.primaryRunning == true }
        XCTAssertFalse(old.snapshot.isActive)
        XCTAssertEqual(model.config.memMiB, 8192)
        XCTAssertEqual(library.selectedID, other.slug)
        XCTAssertEqual(backend.calls[2], 0)
        XCTAssertEqual(backend.calls[3], 0)
        model.stop()
        await f.wait { !model.lifecycleBusy && !record.descriptor.isActive }
        XCTAssertTrue(library.dismissRetainedControl(record.id))
    }

}
