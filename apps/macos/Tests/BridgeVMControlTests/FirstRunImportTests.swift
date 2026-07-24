import XCTest
@testable import BridgeVMControl

final class FirstRunImportTests: XCTestCase {
    private func makeDisk(_ url: URL, bytes: Int = 1) throws {
        try Data(repeating: 0, count: bytes).write(to: url)
    }

    private func makeVars(_ url: URL, bytes: UInt64 = 64 * 1024 * 1024) throws {
        FileManager.default.createFile(atPath: url.path, contents: nil)
        let handle = try FileHandle(forWritingTo: url)
        try handle.truncate(atOffset: bytes)
        try handle.close()
    }

    private func base(_ name: String = #function) -> URL {
        URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("firstrun-" + UUID().uuidString)
    }

    func testValidInputsPass() throws {
        let root = base()
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let disk = root.appendingPathComponent("win.raw")
        let vars = root.appendingPathComponent("vars.fd")
        try makeDisk(disk, bytes: 4096)
        try makeVars(vars)
        let inputs = FirstRunImport.Inputs(
            displayName: "Win11", diskPath: disk.path, varsPath: vars.path,
            vtpmStateDir: nil, memMiB: 6144, cpuCount: 4)
        XCTAssertNil(FirstRunImport.validate(inputs))
    }

    func testEmptyNameRejected() throws {
        let inputs = FirstRunImport.Inputs(
            displayName: "   ", diskPath: "/x", varsPath: "/y",
            vtpmStateDir: nil, memMiB: 6144, cpuCount: 4)
        XCTAssertEqual(FirstRunImport.validate(inputs), .emptyName)
    }

    func testMissingDiskRejected() throws {
        let inputs = FirstRunImport.Inputs(
            displayName: "W", diskPath: "/no/such/disk.raw", varsPath: "/y",
            vtpmStateDir: nil, memMiB: 6144, cpuCount: 4)
        XCTAssertEqual(FirstRunImport.validate(inputs), .diskMissing("/no/such/disk.raw"))
    }

    func testDirectoryAsDiskRejected() throws {
        let root = base()
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let inputs = FirstRunImport.Inputs(
            displayName: "W", diskPath: root.path, varsPath: "/y",
            vtpmStateDir: nil, memMiB: 6144, cpuCount: 4)
        XCTAssertEqual(FirstRunImport.validate(inputs), .diskNotAFile(root.path))
    }

    func testEmptyDiskRejected() throws {
        let root = base()
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let disk = root.appendingPathComponent("win.raw")
        try makeDisk(disk, bytes: 0)
        let inputs = FirstRunImport.Inputs(
            displayName: "W", diskPath: disk.path, varsPath: "/y",
            vtpmStateDir: nil, memMiB: 6144, cpuCount: 4)
        XCTAssertEqual(FirstRunImport.validate(inputs), .diskEmpty(disk.path))
    }

    func testWrongVarsSizeRejected() throws {
        let root = base()
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let disk = root.appendingPathComponent("win.raw")
        let vars = root.appendingPathComponent("vars.fd")
        try makeDisk(disk, bytes: 4096)
        try makeVars(vars, bytes: 1024)
        let inputs = FirstRunImport.Inputs(
            displayName: "W", diskPath: disk.path, varsPath: vars.path,
            vtpmStateDir: nil, memMiB: 6144, cpuCount: 4)
        XCTAssertEqual(
            FirstRunImport.validate(inputs), .varsWrongSize(path: vars.path, bytes: 1024))
    }

    func testVtpmFileInsteadOfDirRejected() throws {
        let root = base()
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let disk = root.appendingPathComponent("win.raw")
        let vars = root.appendingPathComponent("vars.fd")
        let vtpm = root.appendingPathComponent("vtpm-file")
        try makeDisk(disk, bytes: 4096)
        try makeVars(vars)
        try Data([1]).write(to: vtpm)
        let inputs = FirstRunImport.Inputs(
            displayName: "W", diskPath: disk.path, varsPath: vars.path,
            vtpmStateDir: vtpm.path, memMiB: 6144, cpuCount: 4)
        XCTAssertEqual(FirstRunImport.validate(inputs), .vtpmNotADirectory(vtpm.path))
    }

    func testBadResourcesRejected() throws {
        let inputs = FirstRunImport.Inputs(
            displayName: "W", diskPath: "/x", varsPath: "/y",
            vtpmStateDir: nil, memMiB: 512, cpuCount: 4)
        XCTAssertEqual(FirstRunImport.validate(inputs), .badResources(memMiB: 512, cpuCount: 4))
    }

    func testRegisterPlacesBundleAndConfig() throws {
        let root = base()
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let disk = root.appendingPathComponent("win.raw")
        let vars = root.appendingPathComponent("vars.fd")
        let vtpm = root.appendingPathComponent("vtpm")
        try makeDisk(disk, bytes: 8192)
        try makeVars(vars)
        try FileManager.default.createDirectory(at: vtpm, withIntermediateDirectories: true)
        try Data([9]).write(to: vtpm.appendingPathComponent("tpm2-00.permall"))
        try Data().write(to: vtpm.appendingPathComponent(".lock"))
        let library = root.appendingPathComponent("library")
        let inputs = FirstRunImport.Inputs(
            displayName: "My Windows", diskPath: disk.path, varsPath: vars.path,
            vtpmStateDir: vtpm.path, memMiB: 8192, cpuCount: 6)

        let config = try FirstRunImport.register(
            inputs, slug: "my-windows", libraryRoot: library)

        XCTAssertEqual(config.backendKind, BackendKind.hvfEngine.rawValue)
        XCTAssertEqual(config.memMiB, 8192)
        XCTAssertEqual(config.cpuCount, 6)
        let layout = FirstRunImport.BundleLayout(
            bundleURL: URL(fileURLWithPath: config.bundlePath))
        XCTAssertTrue(FileManager.default.fileExists(atPath: layout.diskURL.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: layout.varsURL.path))
        XCTAssertTrue(
            FileManager.default.fileExists(
                atPath: layout.vtpmURL.appendingPathComponent("tpm2-00.permall").path))
        // The .lock is never copied into the imported bundle.
        XCTAssertFalse(
            FileManager.default.fileExists(
                atPath: layout.vtpmURL.appendingPathComponent(".lock").path))
        XCTAssertEqual(config.diskPath, layout.diskURL.path)
    }
}
