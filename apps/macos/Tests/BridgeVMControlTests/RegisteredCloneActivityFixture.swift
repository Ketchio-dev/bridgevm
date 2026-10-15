import XCTest
@testable import BridgeVMControl

@MainActor
final class RegisteredCloneActivityFixture {
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
        let disk = URL(fileURLWithPath: selected.bundlePath).appendingPathComponent("disks/hvf-target.raw")
        try FileManager.default.createDirectory(at: disk.deletingLastPathComponent(), withIntermediateDirectories: true)
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
final class ScheduledCloneCounter { var count = 0 }
