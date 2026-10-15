import XCTest
@testable import BridgeVMControl

final class LibraryNestedBundleDeletionTests: XCTestCase {
    private struct Fixture {
        let root = LibraryDeletionTestDirectory.temporary.appendingPathComponent(UUID().uuidString)
        var library: URL { root.appendingPathComponent("library") }
        var ownerEntry: URL { library.appendingPathComponent("owner") }
        var ownerBundle: URL { ownerEntry.appendingPathComponent("bundle.vmbridge") }
        var importedBundle: URL { library.appendingPathComponent("imported/bundle") }
        let diskContents = Data("synthetic imported disk".utf8)

        func config(_ slug: String, engine: String, bundle: URL) -> VMConfig {
            VMConfig(id: slug, name: slug, displayName: slug, backendKind: engine,
                bootMode: nil, bundlePath: bundle.path, runnerPath: "", launchSpecPath: "",
                handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
                guestName: slug, displayWidth: 800, displayHeight: 600, installPending: false)
        }

        var owner: VMConfig { config("owner", engine: "fast-vz", bundle: ownerBundle) }
        var imported: VMConfig {
            var value = config("imported", engine: "hvf-engine", bundle: importedBundle)
            value.diskPath = FirstRunImport.BundleLayout(bundleURL: importedBundle).diskURL.path
            return value
        }

        func prepare() throws {
            let fm = FileManager.default
            try fm.createDirectory(at: ownerBundle, withIntermediateDirectories: true)
            let disk = FirstRunImport.BundleLayout(bundleURL: importedBundle).diskURL
            try fm.createDirectory(at: disk.deletingLastPathComponent(), withIntermediateDirectories: true)
            try diskContents.write(to: disk)
            // No vTPM state or keys: same-identity relocation only records its receipt.
            XCTAssertTrue(VMLibrary.save(owner, rootURL: library))
            XCTAssertTrue(VMLibrary.save(imported, rootURL: library))
        }

        func moveImported(to parent: URL) throws -> VMConfig {
            let moved = try XCTUnwrap(VMLibrary.moveWindowsHVFBundle(imported, to: parent, rootURL: library))
            XCTAssertEqual(moved.bundlePath, parent.appendingPathComponent("bundle").path)
            XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: library, id: moved.slug), moved)
            XCTAssertFalse(FileManager.default.fileExists(atPath: importedBundle.path))
            return moved
        }

        func remove() { try? FileManager.default.removeItem(at: root) }
    }

    func testDeletingOwnerRefusesToRemoveAnotherRegisteredVMsMovedBundle() throws {
        let f = Fixture(); defer { f.remove() }
        try f.prepare()
        let moved = try f.moveImported(to: f.ownerEntry)
        let disk = URL(fileURLWithPath: try XCTUnwrap(moved.diskPath))
        XCTAssertEqual(try Data(contentsOf: disk), f.diskContents)
        XCTAssertEqual(VMLibrary.deletionTarget(for: f.owner, rootURL: f.library), f.ownerBundle)

        XCTAssertFalse(LibraryDeletionProtection.delete(f.owner, rootURL: f.library))
        XCTAssertTrue(FileManager.default.fileExists(atPath: f.ownerBundle.path))
        XCTAssertEqual(try? Data(contentsOf: disk), f.diskContents)
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: f.library, id: moved.slug), moved)
    }

    func testDeletingOwnerWithUnrelatedMovedBundleStillSucceeds() throws {
        let f = Fixture(); defer { f.remove() }
        try f.prepare()
        let moved = try f.moveImported(to: f.root.appendingPathComponent("unrelated"))
        let disk = URL(fileURLWithPath: try XCTUnwrap(moved.diskPath))

        XCTAssertTrue(LibraryDeletionProtection.delete(f.owner, rootURL: f.library))
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.ownerEntry.path))
        XCTAssertEqual(try Data(contentsOf: disk), f.diskContents)
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: f.library, id: moved.slug), moved)
    }

    @MainActor
    func testLibraryRefusalExplainsHowToPreserveTheOtherVM() async throws {
        let f = Fixture(); defer { f.remove() }
        try f.prepare()
        let moved = try f.moveImported(to: f.ownerEntry)
        let jobs = NestedDeletionJobs()
        let library = LibraryModel(rootURL: f.library, migrateLegacy: false,
            actionScheduler: { jobs.pending.append($0) }, startsModelsAutomatically: false)
        let owner = try XCTUnwrap(library.vms.first { $0.slug == f.owner.slug })
        library.requestDeletion(owner)
        library.confirmDeletion(owner)
        let job = try XCTUnwrap(jobs.pending.first)
        await job()

        let message = try XCTUnwrap(library.deletionError)
        XCTAssertTrue(message.contains("다른 VM"), message)
        XCTAssertTrue(message.contains("다른 위치로 옮기거나 등록 정보를 확인"), message)
        XCTAssertTrue(FileManager.default.fileExists(atPath: f.ownerBundle.path))
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: XCTUnwrap(moved.diskPath))), f.diskContents)

        let rescued = try XCTUnwrap(VMLibrary.moveWindowsHVFBundle(moved,
            to: f.root.appendingPathComponent("rescued"), rootURL: f.library))
        XCTAssertTrue(LibraryDeletionProtection.delete(owner, rootURL: f.library))
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: XCTUnwrap(rescued.diskPath))), f.diskContents)
    }
}

@MainActor
private final class NestedDeletionJobs {
    var pending: [LibraryActionJob] = []
}
