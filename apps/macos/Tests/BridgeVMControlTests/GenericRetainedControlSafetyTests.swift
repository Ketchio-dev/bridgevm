import XCTest
@testable import BridgeVMControl

@MainActor
final class GenericRetainedControlSafetyTests: XCTestCase {
    func testCapturedPaletteStopsExactOwnedControlAfterSameSlugReplacement() async throws {
        let f = try GenericOwnedControlFixture(); defer { f.clean() }
        let original = try f.config("active"), other = try f.config("other")
        f.save(original); f.save(other)
        let library = f.library(), model = library.model(for: original)
        library.selectedID = original.slug
        model.start()
        await f.wait { !model.lifecycleBusy && model.ownedRuntime?.snapshot.primaryRunning == true }
        let owner = try XCTUnwrap(model.ownedRuntime)
        let stop = try XCTUnwrap(library.selectedPaletteCommands(dismiss: {}).first { $0.title == "정지: active" })
        var replacement = original; replacement.name = "replacement"
        f.save(replacement); library.reload()
        let record = try XCTUnwrap(library.retainedControlRecords.first)
        XCTAssertTrue(model.isRetained)
        XCTAssertTrue(library.model(for: replacement) === model, "Active mismatched cache handle keeps its original identity")
        library.selectedID = other.slug
        let navigation = try XCTUnwrap(f.values(LibraryRetainedControlsNavigation.self,
            in: LibrarySidebar(library: library).body).first)
        navigation.selection.wrappedValue = record.id
        let detail = RetainedControlDetailView(library: library, record: record)
        let view = try XCTUnwrap(f.values(RetainedGenericControlView.self, in: detail.body).first)
        XCTAssertTrue(view.model === model)
        XCTAssertTrue(model.retainedStopAvailable)
        XCTAssertFalse(library.dismissRetainedControl(record.id))
        stop.action()
        await f.wait { !model.lifecycleBusy && !owner.snapshot.isActive }
        XCTAssertEqual(f.backends[0].calls[3], 0)
        view.dismiss()
        XCTAssertTrue(library.retainedControlRecords.isEmpty)
    }

    func testStaleUnownedPaletteAndGuestActionsRefuseAfterReplacement() async throws {
        let f = try GenericOwnedControlFixture(); defer { f.clean() }
        let original = try f.config("active"); f.save(original)
        let library = f.library(), model = library.model(for: original)
        library.selectedID = original.slug; model.running = true
        let stop = try XCTUnwrap(library.selectedPaletteCommands(dismiss: {}).first { $0.title == "정지: active" })
        var changed = original; changed.name = "changed"
        f.save(changed); library.reload()
        stop.action()
        model.terminalInput = "synthetic command"
        model.runTerminalCommand(); model.installPackages(["git"], label: "test")
        model.start(); model.applyResources()
        XCTAssertFalse(model.retainedStopAvailable)
        XCTAssertEqual(f.backends[0].calls, [0, 0, 0, 0, 0])
        XCTAssertTrue(library.model(for: changed) === model)
        let shared = try f.config("shared", bundle: URL(fileURLWithPath: original.bundlePath))
        f.save(shared); library.reload()
        let other = library.model(for: shared)
        other.running = true; other.stop()
        XCTAssertEqual(f.backends[1].calls, [0, 0, 0, 0, 0])
        model.running = false // Observed terminal state is final for this retained object.
        model.refreshStatus()
        XCTAssertTrue(model.retainedObservationEnded)
        XCTAssertFalse(model.running)
        XCTAssertEqual(f.backends[0].calls[1], 0)
    }

    func testMissingFastVZBundleProtectsNestedDiskWorkAndDestructiveActions() throws {
        let f = try GenericOwnedControlFixture(); defer { f.clean() }
        let active = try f.config("active"), unrelated = try f.config("unrelated", kind: "hvf-engine")
        f.save(active); f.save(unrelated)
        let library = f.library(), model = library.model(for: active)
        model.running = true
        try FileManager.default.removeItem(at: f.root.appendingPathComponent("active/vm.json"))
        library.reload()
        var nested = try f.config("nested", kind: "hvf-engine", bundle: URL(fileURLWithPath: active.bundlePath).appendingPathComponent("disks"))
        nested.diskPath = nested.bundlePath + "/root.raw"
        try Data("owned fixture".utf8).write(to: URL(fileURLWithPath: nested.diskPath!))
        f.save(nested); library.reload()
        XCTAssertNotNil(library.genericRetainedWorkRefusal(for: nested))
        XCTAssertNotNil(library.genericRetainedFileRefusal(for: nested, action: .move))
        XCTAssertNotNil(library.genericRetainedFileRefusal(for: nested, action: .clone))
        XCTAssertNil(library.genericRetainedWorkRefusal(for: unrelated))
        XCTAssertNil(library.genericRetainedFileRefusal(for: unrelated, action: .move))
        XCTAssertFalse(library.admitLibraryAction(nested, action: .move))
        model.running = false
        XCTAssertNil(library.genericRetainedFileRefusal(for: nested, action: .move))
    }

    func testMissingControlProtectsCrossSlugEntryDeletion() throws {
        let f = try GenericOwnedControlFixture(); defer { f.clean() }
        var active = try f.config("active"), other = try f.config("other")
        active.handoffPath = f.root.appendingPathComponent("other/accepted-handoff.json").path
        f.save(active); f.save(other)
        let library = f.library(), model = library.model(for: active)
        model.running = true
        try FileManager.default.removeItem(at: f.root.appendingPathComponent("active/vm.json"))
        library.reload()
        XCTAssertNotNil(library.genericRetainedFileRefusal(for: other, action: .deletion))
        XCTAssertFalse(library.admitLibraryAction(other, action: .deletion))
        XCTAssertTrue(FileManager.default.fileExists(atPath: f.root.appendingPathComponent("other/vm.json").path))
    }
    func testCloneRefusesRetainedWriterOnSharedExternalDisk() throws {
        let f = try GenericOwnedControlFixture(); defer { f.clean() }
        var active = try f.config("active", kind: "qemu-compat")
        var selected = try f.config("selected", kind: "hvf-engine")
        var disjoint = try f.config("disjoint", kind: "hvf-engine")
        let disk = f.root.appendingPathComponent("shared.raw")
        let otherDisk = f.root.appendingPathComponent("other.raw")
        try Data("synthetic source".utf8).write(to: disk)
        try Data("independent source".utf8).write(to: otherDisk)
        active.diskPath = disk.path; selected.diskPath = disk.path; disjoint.diskPath = otherDisk.path
        f.save(active); f.save(selected); f.save(disjoint)
        let library = f.library(), model = library.model(for: active)
        model.running = true
        try FileManager.default.removeItem(at: f.root.appendingPathComponent("active/vm.json"))
        library.reload()
        XCTAssertNotNil(library.genericRetainedWorkRefusal(for: selected))
        XCTAssertNotNil(library.genericRetainedFileRefusal(for: selected, action: .clone))
        XCTAssertFalse(library.admitLibraryAction(selected, action: .clone))
        XCTAssertNil(library.genericRetainedFileRefusal(for: disjoint, action: .clone))
        XCTAssertEqual(try Data(contentsOf: disk), Data("synthetic source".utf8))
    }

}
