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

    /// The ID becomes the library folder name; the Create sheet refuses one over 200 bytes.
    func testNameWhoseIDExceedsTheFileNameLimitRejected() throws {
        let root = base()
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let disk = root.appendingPathComponent("win.raw")
        try makeDisk(disk, bytes: 4096)
        let name = String(repeating: "가", count: 100) // 100 characters, a 300-byte ID
        let inputs = FirstRunImport.Inputs(
            displayName: name, diskPath: disk.path, vtpmStateDir: nil, memMiB: 6144, cpuCount: 4)
        XCTAssertEqual(FirstRunImport.validate(inputs), .invalidName)
        XCTAssertEqual(FirstRunImport.validate(inputs)?.description, VMLibrary.vmNameRule)
    }

    func testWindowsInstallerISORejected() throws {
        let root = base()
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let iso = root.appendingPathComponent("Win11_Arm64.iso")
        var bytes = Data(count: 0x8800)
        bytes[0x8000] = 1 // primary volume descriptor
        bytes.replaceSubrange(0x8001..<0x8006, with: Data("CD001".utf8))
        try bytes.write(to: iso)
        let inputs = FirstRunImport.Inputs(
            displayName: "W", diskPath: iso.path, vtpmStateDir: nil, memMiB: 6144, cpuCount: 4)
        XCTAssertEqual(FirstRunImport.validate(inputs)?.description,
            "This is a Windows installer ISO. Use Install Windows instead.")
    }

    func testMissingDiskRejected() throws {
        let inputs = FirstRunImport.Inputs(
            displayName: "W", diskPath: "/no/such/disk.raw", varsPath: "/y",
            vtpmStateDir: nil, memMiB: 6144, cpuCount: 4)
        XCTAssertEqual(FirstRunImport.validate(inputs), .media("Choose a readable disk file with Windows installed."))
    }

    func testDirectoryAsDiskRejected() throws {
        let root = base()
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let inputs = FirstRunImport.Inputs(
            displayName: "W", diskPath: root.path, varsPath: "/y",
            vtpmStateDir: nil, memMiB: 6144, cpuCount: 4)
        XCTAssertEqual(FirstRunImport.validate(inputs), .media("Choose a readable disk file with Windows installed."))
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
        XCTAssertEqual(FirstRunImport.validate(inputs), .media("The Windows disk is empty."))
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
        XCTAssertEqual(FirstRunImport.validate(inputs)?.description,
            "The UEFI variables file must be exactly 64 MiB. Choose the one that came with this Windows disk, or leave it empty.")
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
        try makeDisk(disk, bytes: 8192)
        try makeVars(vars)
        let library = root.appendingPathComponent("library")
        let inputs = FirstRunImport.Inputs(
            displayName: "My Windows", diskPath: disk.path, varsPath: vars.path,
            vtpmStateDir: nil, memMiB: 8192, cpuCount: 6)

        let config = try FirstRunImport.register(
            inputs, slug: "my-windows", libraryRoot: library, snapshotHelper: HvfMediaImportTestSupport.helper)

        XCTAssertEqual(config.backendKind, BackendKind.hvfEngine.rawValue)
        XCTAssertEqual(config.memMiB, 8192)
        XCTAssertEqual(config.cpuCount, 6)
        let layout = FirstRunImport.BundleLayout(
            bundleURL: URL(fileURLWithPath: config.bundlePath))
        XCTAssertTrue(FileManager.default.fileExists(atPath: layout.diskURL.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: layout.varsURL.path))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: layout.vtpmURL.path), [])
        XCTAssertEqual(config.diskPath, layout.diskURL.path)
    }
}
