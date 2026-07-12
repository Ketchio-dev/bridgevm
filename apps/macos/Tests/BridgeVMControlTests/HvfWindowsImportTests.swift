import XCTest
@testable import BridgeVMControl

final class HvfWindowsImportTests: XCTestCase {
    func testImportsInstalledDiskAndMatchingVarsAsReadyVM() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let sourceDir = temp.appendingPathComponent("source", isDirectory: true)
        let storage = temp.appendingPathComponent("library", isDirectory: true)
        try FileManager.default.createDirectory(at: sourceDir, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: storage, withIntermediateDirectories: true)
        let sourceDisk = sourceDir.appendingPathComponent("installed.raw")
        let sourceVars = sourceDir.appendingPathComponent("vars.fd")
        let diskBytes = Data((0..<4096).map { UInt8($0 % 251) })
        let varsBytes = Data("matching-uefi-vars".utf8)
        try diskBytes.write(to: sourceDisk)
        try varsBytes.write(to: sourceVars)
        let varsHandle = try FileHandle(forWritingTo: sourceVars)
        try varsHandle.truncate(atOffset: VMLibrary.windowsHVFVarsBytes)
        try varsHandle.close()

        let config = try XCTUnwrap(VMLibrary.createWindowsHVF(
            name: "Imported HVF \(UUID().uuidString)",
            targetDiskPath: sourceDisk.path,
            varsPath: sourceVars.path,
            storageDir: storage,
            width: 1920,
            height: 1080,
            persist: false
        ))

        XCTAssertEqual(config.backendKind, "hvf-engine")
        XCTAssertEqual(config.bootMode, "windows-hvf")
        XCTAssertEqual(config.installPending, false)
        XCTAssertEqual(config.memMiB, 6144)
        XCTAssertEqual(config.cpuCount, 4)
        XCTAssertEqual(config.displayWidth, 1920)
        XCTAssertEqual(config.displayHeight, 1080)
        let importedDisk = URL(fileURLWithPath: try XCTUnwrap(config.diskPath))
        let importedHandle = try FileHandle(forReadingFrom: importedDisk)
        defer { try? importedHandle.close() }
        XCTAssertEqual(try importedHandle.read(upToCount: diskBytes.count), diskBytes)
        let importedSize = try XCTUnwrap(
            (try FileManager.default.attributesOfItem(atPath: importedDisk.path)[.size] as? NSNumber)?.uint64Value
        )
        XCTAssertEqual(importedSize, VMLibrary.minimumImportedWindowsHVFDiskGiB * 1024 * 1024 * 1024)
        let importedVars = URL(fileURLWithPath: config.bundlePath).appendingPathComponent("metadata/hvf-vars.fd")
        let importedVarsHandle = try FileHandle(forReadingFrom: importedVars)
        XCTAssertEqual(try importedVarsHandle.read(upToCount: varsBytes.count), varsBytes)
        try importedVarsHandle.close()
        XCTAssertEqual(
            (try FileManager.default.attributesOfItem(atPath: importedVars.path)[.size] as? NSNumber)?.uint64Value,
            VMLibrary.windowsHVFVarsBytes
        )
        XCTAssertTrue(FileManager.default.fileExists(atPath: config.bundlePath + "/metadata/hvf.ctl"))
        XCTAssertTrue(FileManager.default.fileExists(atPath: config.bundlePath + "/metadata/hvf-grow-pending"))
        XCTAssertEqual(try Data(contentsOf: sourceDisk), diskBytes)
        XCTAssertEqual(
            (try FileManager.default.attributesOfItem(atPath: sourceVars.path)[.size] as? NSNumber)?.uint64Value,
            VMLibrary.windowsHVFVarsBytes
        )
    }

    func testRejectsWrongVarsSizeAndContainerDiskFormats() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let raw = temp.appendingPathComponent("installed.raw")
        let vars = temp.appendingPathComponent("vars.fd")
        try Data([1]).write(to: raw)
        try Data([2]).write(to: vars)
        XCTAssertTrue(VMLibrary.windowsHVFImportError(targetDiskPath: raw.path, varsPath: vars.path)?.contains("64 MiB") == true)

        let varsHandle = try FileHandle(forWritingTo: vars)
        try varsHandle.truncate(atOffset: VMLibrary.windowsHVFVarsBytes)
        try varsHandle.close()
        try Data([0x51, 0x46, 0x49, 0xfb, 0, 0, 0, 3]).write(to: raw)
        XCTAssertTrue(VMLibrary.windowsHVFImportError(targetDiskPath: raw.path, varsPath: vars.path)?.contains("QCOW2") == true)
        try Data("vhdxfile".utf8).write(to: raw)
        XCTAssertTrue(VMLibrary.windowsHVFImportError(targetDiskPath: raw.path, varsPath: vars.path)?.contains("VHDX") == true)
    }

    func testRejectsMissingImportMediaWithoutCreatingVMDirectory() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let storage = temp.appendingPathComponent("library", isDirectory: true)
        try FileManager.default.createDirectory(at: storage, withIntermediateDirectories: true)

        let config = VMLibrary.createWindowsHVF(
            name: "Missing HVF \(UUID().uuidString)",
            targetDiskPath: temp.appendingPathComponent("missing.raw").path,
            varsPath: temp.appendingPathComponent("missing.fd").path,
            storageDir: storage,
            persist: false
        )

        XCTAssertNil(config)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: storage.path), [])
    }

    private func makeTempDir() throws -> URL {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }
}
