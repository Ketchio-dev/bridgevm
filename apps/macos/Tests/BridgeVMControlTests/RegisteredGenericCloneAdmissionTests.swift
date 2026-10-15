import XCTest
@testable import BridgeVMControl

@MainActor
final class RegisteredGenericCloneAdmissionTests: XCTestCase {
    func testRegisteredActiveFastVZRefusesSharedDiskCloneBeforeScheduling() throws {
        try checkCloneAdmission(shared: true)
    }

    func testRegisteredActiveFastVZPermitsDistinctDiskCloneScheduling() throws {
        try checkCloneAdmission(shared: false)
    }

    private func checkCloneAdmission(shared: Bool) throws {
        let fixture = try GenericOwnedControlFixture()
        defer { fixture.clean() }
        var active = try fixture.config("active", kind: "fast-vz")
        var selected = try fixture.config("selected", kind: "hvf-engine")
        let disk = URL(fileURLWithPath: selected.bundlePath).appendingPathComponent("disks/hvf-target.raw")
        let otherDisk = URL(fileURLWithPath: active.bundlePath).appendingPathComponent("disks/root.raw")
        for path in [disk, otherDisk] {
            try FileManager.default.createDirectory(at: path.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data("synthetic raw source".utf8).write(to: path)
        }
        selected.diskPath = disk.path
        active.diskPath = shared ? disk.path : otherDisk.path
        fixture.save(active)
        fixture.save(selected)
        var scheduled = 0
        let library = LibraryModel(rootURL: fixture.root, migrateLegacy: false,
            actionScheduler: { _ in scheduled += 1 }, startsModelsAutomatically: false,
            modelFactory: { config in
                let backend = GenericOwnedControlBackend()
                backend.setExternalRunning(config.slug == active.slug)
                return ControlModel(config: config, backend: backend, startsAutomatically: false)
            })
        let model = library.model(for: active)
        model.running = true // Synthetic observed runtime; no guest or external process is launched.
        _ = library.model(for: selected)
        XCTAssertTrue(library.vms.contains(active))
        XCTAssertTrue(library.vms.contains(selected))
        XCTAssertTrue(model.hasActiveRuntimeWork)
        XCTAssertTrue(library.retainedControlRecords.isEmpty)
        let original = try Data(contentsOf: disk)
        library.cloneWindowsHVF(selected, name: "copy")
        XCTAssertEqual(scheduled, shared ? 0 : 1)
        XCTAssertEqual(library.cloningSlugs.contains(selected.slug), !shared)
        if shared { XCTAssertNotNil(library.cloneError) }
        else { XCTAssertNil(library.cloneError) }
        XCTAssertEqual(try Data(contentsOf: disk), original)
        XCTAssertTrue(FileManager.default.fileExists(atPath: fixture.root.appendingPathComponent("active/vm.json").path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: fixture.root.appendingPathComponent("selected/vm.json").path))
    }
}
