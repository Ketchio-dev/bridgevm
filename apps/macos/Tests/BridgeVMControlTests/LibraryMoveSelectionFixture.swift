import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryMoveSelectionFixture {
    let assets: LibraryMoveSelectionAssets
    let source: VMConfig
    let other: VMConfig
    let library: LibraryModel
    let otherModel: ControlModel
    let sourceModel: ControlModel
    private let sourceBackend: GenericOwnedControlBackend
    private let otherBackend: GenericOwnedControlBackend
    private let otherRegistration: Data
    private let jobs: LibraryMoveSelectionJobs

    init() throws {
        let assets = try LibraryMoveSelectionAssets()
        self.assets = assets
        let source = try assets.configuration("source", kind: "hvf-engine")
        let other = try assets.configuration("other", kind: "fast-vz")
        self.source = source; self.other = other
        otherRegistration = try Data(contentsOf: assets.libraryRoot.appendingPathComponent("other/vm.json"))
        let sourceBackend = GenericOwnedControlBackend(), otherBackend = GenericOwnedControlBackend()
        for backend in [sourceBackend, otherBackend] {
            backend.plan = nil
            backend.setExternalRunning(false)
        }
        self.sourceBackend = sourceBackend; self.otherBackend = otherBackend
        let jobs = LibraryMoveSelectionJobs()
        self.jobs = jobs
        library = LibraryModel(rootURL: assets.libraryRoot, migrateLegacy: false,
            actionScheduler: { jobs.pending.append($0) }, startsModelsAutomatically: false,
            modelFactory: { config in
                ControlModel(config: config, backend: config.slug == source.slug ? sourceBackend : otherBackend,
                             startsAutomatically: false)
            })
        otherModel = library.model(for: other)
        sourceModel = library.model(for: source)
        library.selectedID = source.slug
    }

    func admit() throws {
        library.moveWindowsHVFBundle(source, to: assets.destinationParent)
        XCTAssertEqual(jobs.pending.count, 1)
        XCTAssertTrue(library.movingSlugs.contains(source.slug))
        XCTAssertNil(library.moveError)
        XCTAssertTrue(library.retainedControlRecords.isEmpty)
        try assets.assertReadable(source)
    }

    func finish() async throws {
        let job = try XCTUnwrap(jobs.pending.first)
        jobs.pending.removeAll()
        try await assets.execute(job)
        XCTAssertTrue(library.movingSlugs.isEmpty)
        try assets.assertHelperPreserved()
        try assets.assertReadable(other)
        XCTAssertEqual(try Data(contentsOf: assets.libraryRoot.appendingPathComponent("other/vm.json")), otherRegistration)
        XCTAssertTrue(library.model(for: other) === otherModel)
        for backend in [sourceBackend, otherBackend] {
            let counts = backend.calls
            for index in [0, 2, 3, 4] { XCTAssertEqual(counts[index], 0) }
        }
    }

    func assertMoved() throws {
        XCTAssertNil(library.moveError)
        let moved = try XCTUnwrap(library.vms.first { $0.slug == source.slug })
        XCTAssertEqual(moved.bundlePath, assets.destination.path)
        XCTAssertFalse(FileManager.default.fileExists(atPath: source.bundlePath))
        XCTAssertFalse(library.ownsControlModel(sourceModel, for: source))
        XCTAssertFalse(VMRelocationJournal.isPending(moved, rootURL: assets.libraryRoot))
        try assets.assertReadable(moved)
        let receipts = try FileManager.default.contentsOfDirectory(at: assets.destination
            .appendingPathComponent("metadata/vtpm-lifecycle"), includingPropertiesForKeys: nil)
        XCTAssertEqual(receipts.count, 1)
        let receipt = try XCTUnwrap(receipts.first)
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: receipt)) as? [String: Any])
        XCTAssertEqual(object["operation"] as? String, "move")
    }

    func occupyDestination() throws -> URL {
        try FileManager.default.createDirectory(at: assets.destination, withIntermediateDirectories: true)
        let sentinel = assets.destination.appendingPathComponent("keep.txt")
        try Data("owned occupied destination".utf8).write(to: sentinel, options: .withoutOverwriting)
        return sentinel
    }
}

@MainActor
private final class LibraryMoveSelectionJobs { var pending: [LibraryActionJob] = [] }
