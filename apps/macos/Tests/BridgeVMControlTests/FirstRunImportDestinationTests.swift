import XCTest
@testable import BridgeVMControl

final class FirstRunImportDestinationTests: XCTestCase {
    private struct Fixture {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("first-run-destination-" + UUID().uuidString)
        var library: URL { root.appendingPathComponent("library") }
        var disk: URL { root.appendingPathComponent("source.raw") }
        var vars: URL { root.appendingPathComponent("source.fd") }

        init() throws {
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
            try Data([1, 2, 3, 4]).write(to: disk)
            try Data([5, 6, 7, 8]).write(to: vars)
        }

        func register(_ slug: String, disk sourceDisk: URL? = nil, vars sourceVars: URL? = nil,
                      fileManager: FileManager = .default) throws -> FirstRunImport.BundleLayout {
            let config = try FirstRunImport.register(
                .init(displayName: slug, diskPath: (sourceDisk ?? disk).path,
                      varsPath: (sourceVars ?? vars).path,
                      vtpmStateDir: nil, memMiB: 4096, cpuCount: 2),
                slug: slug, libraryRoot: library, fileManager: fileManager, snapshotHelper: HvfMediaImportTestSupport.helper)
            return FirstRunImport.BundleLayout(bundleURL: URL(fileURLWithPath: config.bundlePath))
        }

        func remove() { try? FileManager.default.removeItem(at: root) }
    }

    func testDuplicateSlugPreservesExistingDiskAndVars() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let existing = try fixture.register("existing")
        try Data([9, 9, 9, 9]).write(to: fixture.disk)
        try Data([8, 8, 8, 8]).write(to: fixture.vars)
        XCTAssertThrowsError(try fixture.register("existing"))
        XCTAssertEqual(try Data(contentsOf: existing.diskURL), Data([1, 2, 3, 4]))
        XCTAssertEqual(try Data(contentsOf: existing.varsURL), Data([5, 6, 7, 8]))
    }

    func testSelfImportPreservesItsSelectedSourceFiles() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let existing = try fixture.register("self-import")
        XCTAssertThrowsError(try fixture.register(
            "self-import", disk: existing.diskURL, vars: existing.varsURL))
        XCTAssertEqual(try Data(contentsOf: existing.diskURL), Data([1, 2, 3, 4]))
        XCTAssertEqual(try Data(contentsOf: existing.varsURL), Data([5, 6, 7, 8]))
    }

    func testVarsCopyFailureRemovesOnlyNewDestination() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let existing = try fixture.register("neighbor")
        let missing = fixture.root.appendingPathComponent("missing.fd")
        XCTAssertThrowsError(try fixture.register("failed", vars: missing))
        XCTAssertFalse(FileManager.default.fileExists(
            atPath: fixture.library.appendingPathComponent("failed").path))
        XCTAssertEqual(try Data(contentsOf: fixture.disk), Data([1, 2, 3, 4]))
        XCTAssertEqual(try Data(contentsOf: fixture.vars), Data([5, 6, 7, 8]))
        XCTAssertEqual(try Data(contentsOf: existing.diskURL), Data([1, 2, 3, 4]))
        XCTAssertEqual(try Data(contentsOf: existing.varsURL), Data([5, 6, 7, 8]))
        _ = try fixture.register("failed")
    }

    func testInvalidSlugDoesNotCreateOrEscapeLibrary() throws {
        for slug in ["", ".", "..", "../escape", "nested/name", "unsafe\nname",
                     String(repeating: "a", count: VMLibrary.maximumVMSlugBytes + 1)] {
            let fixture = try Fixture()
            defer { fixture.remove() }
            XCTAssertThrowsError(try fixture.register(slug), "slug: \(slug.debugDescription)")
            XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.library.path))
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: fixture.root.path).sorted(),
                           ["source.fd", "source.raw"])
        }
    }

    func testExistingSymlinkDestinationDoesNotModifyTarget() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let existing = try fixture.register("real")
        let alias = fixture.library.appendingPathComponent("alias")
        try FileManager.default.createSymbolicLink(
            at: alias, withDestinationURL: fixture.library.appendingPathComponent("real"))
        try Data([9, 9, 9, 9]).write(to: fixture.disk)
        XCTAssertThrowsError(try fixture.register("alias"))
        XCTAssertEqual(try Data(contentsOf: existing.diskURL), Data([1, 2, 3, 4]))
        XCTAssertEqual(try Data(contentsOf: existing.varsURL), Data([5, 6, 7, 8]))
        XCTAssertEqual(try FileManager.default.destinationOfSymbolicLink(atPath: alias.path),
                       fixture.library.appendingPathComponent("real").path)
    }

    func testMoveFailureDoesNotRemoveReplacementDirectory() throws {
        final class ReplacingFileManager: FileManager, @unchecked Sendable {
            var beforeMove: ((String, String) throws -> Void)?
            override func moveItem(atPath source: String, toPath destination: String) throws {
                try beforeMove?(source, destination)
                try super.moveItem(atPath: source, toPath: destination)
            }
        }
        let fixture = try Fixture()
        defer { fixture.remove() }
        let owned = fixture.library.appendingPathComponent("replaced")
        let moved = fixture.root.appendingPathComponent("moved-reservation")
        let replacement = owned.appendingPathComponent("unrelated.txt")
        let manager = ReplacingFileManager()
        manager.beforeMove = { source, _ in
            guard URL(fileURLWithPath: source).lastPathComponent == "vars.fd" else { return }
            try FileManager.default.moveItem(at: owned, to: moved)
            try FileManager.default.createDirectory(at: owned, withIntermediateDirectories: false)
            try Data([9]).write(to: replacement)
            throw CocoaError(.fileReadUnknown)
        }
        XCTAssertThrowsError(try fixture.register("replaced", fileManager: manager))
        XCTAssertEqual(try Data(contentsOf: replacement), Data([9]))
        XCTAssertEqual(try Data(contentsOf: moved.appendingPathComponent("bundle/disks/hvf-target.raw")),
                       Data([1, 2, 3, 4]))
        XCTAssertEqual(try Data(contentsOf: fixture.disk), Data([1, 2, 3, 4]))
        XCTAssertEqual(try Data(contentsOf: fixture.vars), Data([5, 6, 7, 8]))
    }
}
