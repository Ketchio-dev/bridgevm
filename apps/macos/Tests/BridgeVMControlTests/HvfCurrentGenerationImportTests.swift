import XCTest
@testable import BridgeVMControl

final class HvfCurrentGenerationImportTests: XCTestCase {
    private var helper: URL { HvfMediaImportTestSupport.helper }

    private struct Fixture {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("hvf-current-import-" + UUID().uuidString)
        var disk: URL { root.appendingPathComponent("disk.raw") }
        var vars: URL { root.appendingPathComponent("vars.fd") }
        var snapshot: URL { root.appendingPathComponent("snapshot") }

        init() throws {
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
            try Data("disk-old".utf8).write(to: disk)
            try Data("vars-old".utf8).write(to: vars)
            let handle = try FileHandle(forWritingTo: vars)
            try handle.truncate(atOffset: FirstRunImport.requiredVarsBytes)
            try handle.close()
        }

        func remove() { try? FileManager.default.removeItem(at: root) }
        func inputs(_ name: String) -> FirstRunImport.Inputs {
            .init(displayName: name, diskPath: disk.path, varsPath: vars.path,
                  vtpmStateDir: nil, memMiB: 4096, cpuCount: 2)
        }
    }

    private func invoke(_ arguments: [String]) throws {
        let process = Process()
        let output = Pipe()
        process.executableURL = helper
        process.arguments = arguments
        process.standardOutput = output
        process.standardError = output
        try process.run()
        let result = output.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        XCTAssertEqual(process.terminationStatus, 0, String(decoding: result, as: UTF8.self))
        if process.terminationStatus != 0 { throw CocoaError(.fileReadUnknown) }
    }

    private func overwrite(_ url: URL, prefix: String) throws {
        let handle = try FileHandle(forWritingTo: url)
        defer { try? handle.close() }
        try handle.write(contentsOf: Data(prefix.utf8))
        try handle.synchronize()
    }

    private func prefix(_ url: URL) throws -> String {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        return String(decoding: try handle.read(upToCount: 8) ?? Data(), as: UTF8.self)
    }

    private func restoredFixture() throws -> Fixture {
        let fixture = try Fixture()
        do {
            let quota = String(FirstRunImport.requiredVarsBytes + 8)
            try invoke(["create", fixture.disk.path, fixture.vars.path, fixture.snapshot.path, "audit", quota])
            try overwrite(fixture.disk, prefix: "disk-new")
            try overwrite(fixture.vars, prefix: "vars-new")
            try invoke(["restore", fixture.snapshot.path, fixture.disk.path, fixture.vars.path])
            // Query the selected pair through the real native operation, without
            // reproducing managed directory identity or selection in Swift.
            let selected = fixture.root.appendingPathComponent("selected-check")
            try invoke(["create", fixture.disk.path, fixture.vars.path, selected.path, "audit", quota])
            XCTAssertEqual(try prefix(selected.appendingPathComponent("disk.raw")), "disk-old")
            XCTAssertEqual(try prefix(selected.appendingPathComponent("vars.fd")), "vars-old")
            try assertOriginals(fixture)
            return fixture
        } catch { fixture.remove(); throw error }
    }

    private func assertOriginals(_ fixture: Fixture) throws {
        XCTAssertEqual(try prefix(fixture.disk), "disk-new")
        XCTAssertEqual(try prefix(fixture.vars), "vars-new")
    }

    func testBothImportEntryPointsCopySelectedGeneration() throws {
        let fixture = try restoredFixture()
        defer { fixture.remove() }
        XCTAssertNil(FirstRunImport.validate(fixture.inputs("first")))
        let first = try FirstRunImport.register(fixture.inputs("first"), slug: "first",
            libraryRoot: fixture.root.appendingPathComponent("first-library"), snapshotHelper: helper)
        let firstLayout = FirstRunImport.BundleLayout(bundleURL: URL(fileURLWithPath: first.bundlePath))
        XCTAssertEqual(try prefix(firstLayout.diskURL), "disk-old")
        XCTAssertEqual(try prefix(firstLayout.varsURL), "vars-old")
        let ordinary = try XCTUnwrap(VMLibrary.createWindowsHVF(name: "ordinary", targetDiskPath: fixture.disk.path,
            varsPath: fixture.vars.path, libraryRoot: fixture.root.appendingPathComponent("ordinary-library"),
            persist: false, snapshotHelper: helper))
        let ordinaryLayout = FirstRunImport.BundleLayout(bundleURL: URL(fileURLWithPath: ordinary.bundlePath))
        XCTAssertEqual(try prefix(ordinaryLayout.diskURL), "disk-old")
        XCTAssertEqual(try prefix(ordinaryLayout.varsURL), "vars-old")
        try assertOriginals(fixture)
    }

    func testBothImportEntryPointsRefuseHeldNativeOwnership() throws {
        let fixture = try restoredFixture()
        defer { fixture.remove() }
        let owner = try HvfMediaLeaseSession(executable: helper, disk: fixture.disk.path, vars: fixture.vars.path)
        defer { owner.abort() }
        XCTAssertThrowsError(try FirstRunImport.register(fixture.inputs("first"), slug: "first",
            libraryRoot: fixture.root.appendingPathComponent("first-library"), snapshotHelper: helper))
        let ordinaryLibrary = fixture.root.appendingPathComponent("ordinary-library")
        XCTAssertNil(VMLibrary.createWindowsHVF(name: "ordinary", targetDiskPath: fixture.disk.path,
            varsPath: fixture.vars.path, libraryRoot: ordinaryLibrary, persist: false, snapshotHelper: helper))
        XCTAssertFalse(FileManager.default.fileExists(
            atPath: fixture.root.appendingPathComponent("first-library/first").path))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: ordinaryLibrary.path), [])
        try assertOriginals(fixture)
        try owner.finish()
    }
}
