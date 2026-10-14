import XCTest
@testable import BridgeVMControl

@MainActor
final class RegisteredGenericCloneActivityTests: XCTestCase {
    func testRegisteredQemuAliasRefusesUntilObservedActivityEnds() throws {
        let fixture = try RegisteredCloneActivityFixture(alias: true)
        defer { fixture.clean() }
        fixture.model.running = true
        fixture.assertClone(admitted: false)
        fixture.model.running = false
        fixture.assertClone(admitted: true)
    }

    func testAcceptedResourceWorkRefusesCloneUntilCompletion() async throws {
        let fixture = try RegisteredCloneActivityFixture()
        defer { fixture.clean() }
        let gate = DispatchSemaphore(value: 0)
        fixture.backend.resourceGate = gate
        defer { gate.signal() }
        fixture.model.pendingMemGiB = 8; fixture.model.pendingCPU = 4
        fixture.model.applyResources()
        await fixture.storage.wait { fixture.backend.didBeginResourceUpdate }
        XCTAssertTrue(fixture.model.hasAcceptedOperation)
        XCTAssertFalse(fixture.model.running)
        fixture.assertClone(admitted: false)
        gate.signal()
        await fixture.storage.wait { !fixture.model.hasAcceptedOperation }
        XCTAssertEqual(fixture.backend.calls[0], 0)
        fixture.assertClone(admitted: true)
    }

    func testOwnedTPMCleanupRefusesCloneUntilTerminal() async throws {
        let fixture = try RegisteredCloneActivityFixture()
        defer { fixture.clean() }
        let child = try GenericOwnedDelayedChildFixture()
        let plan = GenericOwnedLaunchPlan(primary: child.spec("primary", output: .file(child.root.path)),
            tpm: .init(process: child.spec("tpm"), reusePattern: "owned-fixture", launchDelay: 0))
        let group = GenericOwnedRuntime(plan: plan, spawn: child.spawnAfterTPMReady,
            isDependencyRunning: { _ in false })
        defer { group.requestStop(); child.clean() }
        fixture.model.ownedRuntime = group
        try await child.awaitMarker("tpm", "signal")
        XCTAssertEqual(group.snapshot.phase, .stopping)
        XCTAssertTrue(group.snapshot.isActive)
        XCTAssertFalse(group.snapshot.primaryRunning)
        XCTAssertFalse(fixture.model.running)
        XCTAssertFalse(fixture.model.hasAcceptedOperation)
        fixture.assertClone(admitted: false)
        try child.release("tpm")
        await fixture.storage.wait { !group.snapshot.isActive }
        try child.assertReleased("tpm")
        fixture.assertClone(admitted: true)
    }
}

@MainActor
private final class RegisteredCloneActivityFixture {
    let storage: GenericOwnedControlFixture
    let backend: GenericOwnedControlBackend
    let model: ControlModel
    let library: LibraryModel
    let selected: VMConfig
    let disk: URL
    let scheduled: ScheduledCloneCounter

    init(alias: Bool = false) throws {
        let storage = try GenericOwnedControlFixture()
        self.storage = storage
        var active = try storage.config("active", kind: "qemu-compat")
        var selected = try storage.config("selected", kind: "hvf-engine")
        let disk = URL(fileURLWithPath: selected.bundlePath).appendingPathComponent("root.raw")
        try Data("synthetic source".utf8).write(to: disk)
        selected.diskPath = disk.path
        active.diskPath = disk.path
        if alias {
            let link = URL(fileURLWithPath: active.bundlePath).appendingPathComponent("alias.raw")
            try FileManager.default.createSymbolicLink(at: link, withDestinationURL: disk)
            active.diskPath = link.path
        }
        storage.save(active); storage.save(selected)
        let backend = GenericOwnedControlBackend(); backend.setExternalRunning(false)
        let model = ControlModel(config: active, backend: backend, startsAutomatically: false)
        let scheduled = ScheduledCloneCounter()
        let library = LibraryModel(rootURL: storage.root, migrateLegacy: false,
            actionScheduler: { _ in scheduled.count += 1 }, startsModelsAutomatically: false,
            modelFactory: { config in
                if config.slug == active.slug { return model }
                let idle = GenericOwnedControlBackend(); idle.setExternalRunning(false)
                return ControlModel(config: config, backend: idle, startsAutomatically: false)
            })
        storage.models.append(model)
        _ = library.model(for: active); _ = library.model(for: selected)
        self.backend = backend; self.model = model; self.library = library
        self.selected = selected; self.disk = disk; self.scheduled = scheduled
    }

    func assertClone(admitted: Bool) {
        library.cloneError = nil
        library.cloneWindowsHVF(selected, name: "copy")
        XCTAssertEqual(scheduled.count, admitted ? 1 : 0)
        XCTAssertEqual(library.cloningSlugs.contains(selected.slug), admitted)
        XCTAssertEqual(library.cloneError == nil, admitted)
        XCTAssertEqual(try? Data(contentsOf: disk), Data("synthetic source".utf8))
    }

    func clean() { storage.clean() }
}

@MainActor
private final class ScheduledCloneCounter { var count = 0 }
