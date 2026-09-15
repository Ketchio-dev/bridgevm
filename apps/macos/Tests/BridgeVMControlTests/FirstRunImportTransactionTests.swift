import XCTest
@testable import BridgeVMControl

struct FirstRunImportTransactionFixture: Sendable {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("first-run-transaction-" + UUID().uuidString)
    var library: URL { root.appendingPathComponent("library") }
    var entry: URL { library.appendingPathComponent("imported") }
    var disk: URL { root.appendingPathComponent("disk.raw") }
    var vars: URL { root.appendingPathComponent("vars.fd") }

    init() throws {
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        try Data([1, 2, 3, 4]).write(to: disk)
        try Data([5, 6, 7, 8]).write(to: vars)
        let handle = try FileHandle(forWritingTo: vars)
        try handle.truncate(atOffset: FirstRunImport.requiredVarsBytes)
        try handle.close()
    }

    func inputs(_ name: String = "imported") -> FirstRunImport.Inputs {
        .init(displayName: name, diskPath: disk.path, varsPath: vars.path,
              vtpmStateDir: nil, memMiB: 4096, cpuCount: 2)
    }

    func assertPersisted(file: StaticString = #filePath, line: UInt = #line) throws {
        let configs = VMLibrary.list(rootURL: library)
        XCTAssertEqual(configs.map(\.slug), ["imported"], file: file, line: line)
        let config = try XCTUnwrap(configs.first, file: file, line: line)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: XCTUnwrap(config.diskPath))),
            Data([1, 2, 3, 4]), file: file, line: line)
        let layout = FirstRunImport.BundleLayout(bundleURL: URL(fileURLWithPath: config.bundlePath))
        XCTAssertEqual(try Data(contentsOf: layout.varsURL), try Data(contentsOf: vars), file: file, line: line)
        try assertSources(file: file, line: line)
    }

    func assertSources(file: StaticString = #filePath, line: UInt = #line) throws {
        XCTAssertEqual(try Data(contentsOf: disk), Data([1, 2, 3, 4]), file: file, line: line)
        var expected = Data([5, 6, 7, 8])
        expected.count = Int(FirstRunImport.requiredVarsBytes)
        XCTAssertEqual(try Data(contentsOf: vars), expected, file: file, line: line)
    }

    func remove() { try? FileManager.default.removeItem(at: root) }
}

final class FirstRunImportTransactionTests: XCTestCase {
    func testConfigWriteRefusalRemovesPreparedBundle() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        // Three name fields exceed the config limit; the normalized slug is x.
        let inputs = fixture.inputs(String(repeating: "!", count: 350_000) + "x")
        XCTAssertNil(FirstRunImport.validate(inputs))
        let result = FirstRunImportWorker.run(inputs, libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper)
        guard case .failed = result else { return XCTFail("oversized config must refuse publication") }
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.library.appendingPathComponent("x").path))
        try fixture.assertSources()
    }

    func testCommittedImportPersistsIndependentPairAndConfig() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let result = FirstRunImportWorker.run(fixture.inputs(), libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper)
        guard case .committed = result else { return XCTFail("import must commit") }
        try fixture.assertPersisted()
    }

    func testRenameRefusalCleansOnlyPreparedEntry() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let marker = fixture.root.appendingPathComponent("unrelated")
        try Data([9]).write(to: marker)
        var attemptedCommit = false
        let result = FirstRunImportWorker.run(fixture.inputs(), libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper) { config, root in
            do {
                try FileManager.default.createDirectory(at: fixture.entry.appendingPathComponent("vm.json"),
                    withIntermediateDirectories: false)
            } catch { return .notPublished(error) }
            attemptedCommit = true
            return VMLibrary.saveOutcome(config, rootURL: root)
        }
        guard case .failed = result else { return XCTFail("rename onto directory must refuse") }
        XCTAssertTrue(attemptedCommit)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path))
        XCTAssertEqual(try Data(contentsOf: marker), Data([9]))
        try fixture.assertSources()
    }

    func testPublishedConfigAndMediaSurviveDirectorySyncFailure() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let result = FirstRunImportWorker.run(fixture.inputs(), libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper) { config, root in
            VMLibrary.saveOutcome(config, rootURL: root) { data, url in
                VMRegistrationWriter.commit(data, to: url, syncParent: { _ in throw CocoaError(.fileWriteUnknown) })
            }
        }
        guard case .publishedButUnsynced = result else { return XCTFail("publication must be retained") }
        try fixture.assertPersisted()
    }

    func testFailureDoesNotDeleteReplacementOfReservedEntry() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let moved = fixture.root.appendingPathComponent("moved-preparation")
        let marker = fixture.entry.appendingPathComponent("replacement")
        let result = FirstRunImportWorker.run(fixture.inputs(), libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper) { _, _ in
            do {
                try FileManager.default.moveItem(at: fixture.entry, to: moved)
                try FileManager.default.createDirectory(at: fixture.entry, withIntermediateDirectories: false)
                try Data([9]).write(to: marker)
            } catch { return .notPublished(error) }
            return .notPublished(CocoaError(.fileWriteUnknown))
        }
        guard case .failed = result else { return XCTFail("injected refusal must fail") }
        XCTAssertEqual(try Data(contentsOf: marker), Data([9]))
        XCTAssertEqual(try Data(contentsOf: moved.appendingPathComponent("bundle/disks/hvf-target.raw")),
            Data([1, 2, 3, 4]))
        try fixture.assertSources()
    }
}
