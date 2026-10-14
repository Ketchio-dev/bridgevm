import XCTest
@testable import BridgeVMControl

@MainActor
final class ReservedCloneGenericFixture {
    let assets: GenericOwnedControlFixture
    let selected: VMConfig
    let generic: VMConfig
    let disk: URL
    let original: Data
    let registrations: [URL: Data]
    let backend: GenericOwnedControlBackend
    let library: LibraryModel
    let model: ControlModel
    private let jobs: ReservedCloneJobs

    init(kind: String, shared: Bool, alias: Bool = false) throws {
        let assets = try GenericOwnedControlFixture()
        self.assets = assets
        var generic = try assets.config("generic", kind: kind)
        var selected = try assets.config("selected", kind: "hvf-engine")
        disk = URL(fileURLWithPath: selected.bundlePath).appendingPathComponent("disks/hvf-target.raw")
        let ownDisk = URL(fileURLWithPath: generic.bundlePath).appendingPathComponent("disks/root.raw")
        original = Data("owned synthetic clone source".utf8)
        for path in [disk, ownDisk] {
            try FileManager.default.createDirectory(at: path.deletingLastPathComponent(), withIntermediateDirectories: true)
            try original.write(to: path)
        }
        selected.diskPath = disk.path
        generic.diskPath = shared ? disk.path : ownDisk.path
        if alias {
            let link = assets.root.appendingPathComponent("shared-alias.raw")
            try FileManager.default.createSymbolicLink(at: link, withDestinationURL: disk)
            generic.diskPath = link.path
        }
        assets.save(generic); assets.save(selected)
        self.generic = generic; self.selected = selected
        registrations = try Dictionary(uniqueKeysWithValues: [generic, selected].map {
            let url = assets.root.appendingPathComponent($0.slug + "/vm.json")
            return (url, try Data(contentsOf: url))
        })
        let backend = GenericOwnedControlBackend()
        backend.plan = nil // Captures the owned launch request without spawning even a harmless process.
        backend.setExternalRunning(false)
        self.backend = backend
        let jobs = ReservedCloneJobs()
        self.jobs = jobs
        library = LibraryModel(rootURL: assets.root, migrateLegacy: false,
            actionScheduler: { jobs.values.append($0) }, startsModelsAutomatically: false,
            modelFactory: { config in
                let chosen: GenericOwnedControlBackend
                if config.slug == generic.slug { chosen = backend }
                else { chosen = GenericOwnedControlBackend(); chosen.setExternalRunning(false) }
                return ControlModel(config: config, backend: chosen, startsAutomatically: false)
            })
        model = library.model(for: generic)
        _ = library.model(for: selected)
    }

    func admitClone(operation: LibraryCloneOperation? = nil) {
        if let operation { library.cloneWindowsHVF(selected, name: "copy", operation: operation) }
        else { library.cloneWindowsHVF(selected, name: "copy") }
        XCTAssertEqual(jobs.values.count, 1)
        XCTAssertTrue(library.cloningSlugs.contains(selected.slug))
        XCTAssertNil(library.cloneError)
        XCTAssertTrue(library.retainedControlRecords.isEmpty)
    }

    func runClone() throws -> Task<Void, Never> {
        let job = try XCTUnwrap(jobs.values.first)
        jobs.values.removeAll()
        return Task.detached { await job() }
    }

    func finishClone() async throws {
        try await runClone().value
        XCTAssertTrue(library.cloningSlugs.isEmpty)
    }

    func assertUnchanged(ignoringSelectedRegistration: Bool = false) throws {
        XCTAssertEqual(jobs.values.count, 1)
        XCTAssertTrue(library.cloningSlugs.contains(selected.slug))
        XCTAssertEqual(try Data(contentsOf: disk), original)
        for (url, bytes) in registrations {
            if ignoringSelectedRegistration && url.deletingLastPathComponent().lastPathComponent == selected.slug { continue }
            XCTAssertEqual(try Data(contentsOf: url), bytes)
        }
    }

    func clean() { jobs.values.removeAll(); assets.clean() }
}

@MainActor
private final class ReservedCloneJobs { var values: [LibraryActionJob] = [] }
