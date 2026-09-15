import XCTest
@testable import BridgeVMControl

final class HvfMediaImportTests: XCTestCase {
    private struct Fixture {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("hvf-media-import-" + UUID().uuidString)
        var disk: URL { root.appendingPathComponent("source.raw") }
        var vars: URL { root.appendingPathComponent("source.fd") }
        var library: URL { root.appendingPathComponent("library") }

        init() throws {
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
            try Data([1, 2, 3]).write(to: disk)
            try Data([4, 5, 6]).write(to: vars)
        }

        func register(helper: URL) throws {
            _ = try FirstRunImport.register(
                .init(displayName: "Import", diskPath: disk.path, varsPath: vars.path,
                      vtpmStateDir: nil, memMiB: 4096, cpuCount: 2),
                slug: "import", libraryRoot: library, snapshotHelper: helper)
        }

        func remove() { try? FileManager.default.removeItem(at: root) }
    }

    func testMissingOrSymlinkedNativeHelperRefusesAndRollsBack() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let missing = fixture.root.appendingPathComponent("missing-helper")
        let alias = fixture.root.appendingPathComponent("helper-alias")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: HvfMediaImportTestSupport.helper)
        for helper in [missing, alias] {
            XCTAssertThrowsError(try fixture.register(helper: helper))
            XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.library.appendingPathComponent("import").path))
            XCTAssertEqual(try Data(contentsOf: fixture.disk), Data([1, 2, 3]))
            XCTAssertEqual(try Data(contentsOf: fixture.vars), Data([4, 5, 6]))
        }
    }

    func testNativeFailureNeverFallsBackToOriginalCopy() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let helper = fixture.root.appendingPathComponent("refusing-helper")
        try Data("#!/bin/sh\n: > \"$0.ran\"\nexit 7\n".utf8).write(to: helper)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: helper.path)
        XCTAssertThrowsError(try fixture.register(helper: helper.resolvingSymlinksInPath()))
        XCTAssertTrue(FileManager.default.fileExists(atPath: helper.path + ".ran"))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: fixture.library.path), [])
        XCTAssertEqual(try Data(contentsOf: fixture.disk), Data([1, 2, 3]))
        XCTAssertEqual(try Data(contentsOf: fixture.vars), Data([4, 5, 6]))
    }

    func testVerifiedNativeCopyNeverReplacesExistingDestination() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let output = fixture.root.appendingPathComponent("output")
        try FileManager.default.createDirectory(at: output, withIntermediateDirectories: false)
        let disk = output.appendingPathComponent("disk.raw")
        let vars = output.appendingPathComponent("vars.fd")
        try Data([9]).write(to: disk)
        XCTAssertThrowsError(try HvfMediaImport.copy(
            disk: fixture.disk.path, vars: fixture.vars.path, toDisk: disk, toVars: vars,
            helper: HvfMediaImportTestSupport.helper))
        XCTAssertEqual(try Data(contentsOf: disk), Data([9]))
        XCTAssertFalse(FileManager.default.fileExists(atPath: vars.path))
        XCTAssertEqual(try Data(contentsOf: fixture.disk), Data([1, 2, 3]))
        XCTAssertEqual(try Data(contentsOf: fixture.vars), Data([4, 5, 6]))
    }
}
