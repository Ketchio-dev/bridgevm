import XCTest
@testable import BridgeVMControl

final class FirstRunImportIsolationTests: XCTestCase {
    private struct Fixture {
        let root: URL
        let disk: URL
        let vars: URL

        init() throws {
            root = FileManager.default.temporaryDirectory
                .appendingPathComponent("first-run-isolation-" + UUID().uuidString)
            disk = root.appendingPathComponent("source.raw")
            vars = root.appendingPathComponent("source.fd")
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
            try Data([1, 2, 3, 4]).write(to: disk)
            try Data([5, 6, 7, 8]).write(to: vars)
        }

        func register(_ slug: String, sourceDisk: URL? = nil, sourceVars: URL? = nil) throws
            -> FirstRunImport.BundleLayout
        {
            // register's storage behavior needs only tiny files; vars-size
            // validation is independently covered by FirstRunImportTests.
            let config = try FirstRunImport.register(
                .init(displayName: slug, diskPath: (sourceDisk ?? disk).path,
                      varsPath: (sourceVars ?? vars).path,
                      vtpmStateDir: nil, memMiB: 4096, cpuCount: 2),
                slug: slug, libraryRoot: root.appendingPathComponent("library"))
            return FirstRunImport.BundleLayout(bundleURL: URL(fileURLWithPath: config.bundlePath))
        }

        func remove() throws { try FileManager.default.removeItem(at: root) }
    }

    private func overwrite(_ url: URL, with bytes: [UInt8]) throws {
        let handle = try FileHandle(forWritingTo: url)
        defer { try? handle.close() }
        try handle.seek(toOffset: 0)
        // In-place writes expose shared inodes; atomic replacement would hide them.
        try handle.write(contentsOf: Data(bytes))
        try handle.synchronize()
    }

    private func assertIndependent(_ first: URL, _ second: URL) throws {
        let a = try FileManager.default.attributesOfItem(atPath: first.path)
        let b = try FileManager.default.attributesOfItem(atPath: second.path)
        XCTAssertEqual(a[.systemNumber] as? NSNumber, b[.systemNumber] as? NSNumber)
        let firstInode = try XCTUnwrap(a[.systemFileNumber] as? NSNumber)
        let secondInode = try XCTUnwrap(b[.systemFileNumber] as? NSNumber)
        XCTAssertNotEqual(firstInode, secondInode)
    }

    func testImportedWritesPreserveSourceDiskAndVars() throws {
        let fixture = try Fixture()
        defer { try? fixture.remove() }
        let imported = try fixture.register("first")
        XCTAssertEqual(try Data(contentsOf: imported.diskURL), Data([1, 2, 3, 4]))
        XCTAssertEqual(try Data(contentsOf: imported.varsURL), Data([5, 6, 7, 8]))
        try assertIndependent(fixture.disk, imported.diskURL)
        try assertIndependent(fixture.vars, imported.varsURL)

        try overwrite(imported.diskURL, with: [9, 9, 9, 9])
        try overwrite(imported.varsURL, with: [8, 8, 8, 8])
        XCTAssertEqual(try Data(contentsOf: imported.diskURL), Data([9, 9, 9, 9]))
        XCTAssertEqual(try Data(contentsOf: imported.varsURL), Data([8, 8, 8, 8]))
        XCTAssertEqual(try Data(contentsOf: fixture.disk), Data([1, 2, 3, 4]))
        XCTAssertEqual(try Data(contentsOf: fixture.vars), Data([5, 6, 7, 8]))
    }

    func testSourceWritesPreserveImportedDiskAndVars() throws {
        let fixture = try Fixture()
        defer { try? fixture.remove() }
        let imported = try fixture.register("first")
        try overwrite(fixture.disk, with: [3, 3, 3, 3])
        try overwrite(fixture.vars, with: [4, 4, 4, 4])
        XCTAssertEqual(try Data(contentsOf: fixture.disk), Data([3, 3, 3, 3]))
        XCTAssertEqual(try Data(contentsOf: fixture.vars), Data([4, 4, 4, 4]))
        XCTAssertEqual(try Data(contentsOf: imported.diskURL), Data([1, 2, 3, 4]))
        XCTAssertEqual(try Data(contentsOf: imported.varsURL), Data([5, 6, 7, 8]))
    }

    func testSymbolicSourceImportsIndependentDiskBytes() throws {
        let fixture = try Fixture()
        defer { try? fixture.remove() }
        let alias = fixture.root.appendingPathComponent("source-alias.raw")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: fixture.disk)
        let imported = try fixture.register("first", sourceDisk: alias)
        let attributes = try FileManager.default.attributesOfItem(atPath: imported.diskURL.path)
        XCTAssertEqual(attributes[.type] as? FileAttributeType, .typeRegular)
        XCTAssertEqual(try Data(contentsOf: imported.diskURL), Data([1, 2, 3, 4]))
        try assertIndependent(fixture.disk, imported.diskURL)
        try overwrite(imported.diskURL, with: [9, 9, 9, 9])
        XCTAssertEqual(try Data(contentsOf: fixture.disk), Data([1, 2, 3, 4]))
    }

    func testTwoImportsHaveIndependentWritableDiskAndVars() throws {
        let fixture = try Fixture()
        defer { try? fixture.remove() }
        let first = try fixture.register("first")
        let second = try fixture.register("second")
        try assertIndependent(first.diskURL, second.diskURL)
        try assertIndependent(first.varsURL, second.varsURL)
        try overwrite(first.diskURL, with: [9, 8, 7, 6])
        try overwrite(first.varsURL, with: [1, 1, 1, 1])
        XCTAssertEqual(try Data(contentsOf: second.diskURL), Data([1, 2, 3, 4]))
        XCTAssertEqual(try Data(contentsOf: second.varsURL), Data([5, 6, 7, 8]))
        try overwrite(second.diskURL, with: [2, 2, 2, 2])
        try overwrite(second.varsURL, with: [3, 3, 3, 3])
        XCTAssertEqual(try Data(contentsOf: first.diskURL), Data([9, 8, 7, 6]))
        XCTAssertEqual(try Data(contentsOf: first.varsURL), Data([1, 1, 1, 1]))
        XCTAssertEqual(try Data(contentsOf: fixture.disk), Data([1, 2, 3, 4]))
        XCTAssertEqual(try Data(contentsOf: fixture.vars), Data([5, 6, 7, 8]))
    }

    func testSymbolicSourceImportsIndependentVarsBytes() throws {
        let fixture = try Fixture()
        defer { try? fixture.remove() }
        let alias = fixture.root.appendingPathComponent("source-alias.fd")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: fixture.vars)
        let imported = try fixture.register("first", sourceVars: alias)
        let attributes = try FileManager.default.attributesOfItem(atPath: imported.varsURL.path)
        XCTAssertEqual(attributes[.type] as? FileAttributeType, .typeRegular)
        XCTAssertEqual(try Data(contentsOf: imported.varsURL), Data([5, 6, 7, 8]))
        try assertIndependent(fixture.vars, imported.varsURL)
        try overwrite(imported.varsURL, with: [9, 9, 9, 9])
        XCTAssertEqual(try Data(contentsOf: fixture.vars), Data([5, 6, 7, 8]))
        try overwrite(fixture.vars, with: [3, 3, 3, 3])
        XCTAssertEqual(try Data(contentsOf: imported.varsURL), Data([9, 9, 9, 9]))
    }
}
