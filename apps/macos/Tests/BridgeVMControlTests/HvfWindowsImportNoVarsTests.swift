import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfWindowsImportNoVarsTests: XCTestCase {
    func testGPTImportWithoutVarsSeedsCopiedESPAndLeavesSourceUntouched() throws {
        try checkImport(hasGPT: true, suppliedVars: false)
    }

    func testImportWithoutGPTKeepsPlainSeedAndReturnsNotice() throws {
        try checkImport(hasGPT: false, suppliedVars: false)
    }

    func testSuppliedVarsAreByteIdenticalAndLeaveSourceUntouched() throws {
        try checkImport(hasGPT: true, suppliedVars: true)
    }

    func testLiveImportThroughSheetModel() throws {
        let env = ProcessInfo.processInfo.environment
        guard let source = env["BV_IMPORT_SOURCE"], let library = env["BV_IMPORT_LIBRARY"],
              let output = env["BV_IMPORT_OUTPUT"] else { throw XCTSkip("explicit live import paths required") }
        guard case .created(let config)? = VMLibrary.createWindowsHVF(name: "Imported Windows",
            targetDiskPath: source, libraryRoot: URL(fileURLWithPath: library), persist: false,
            snapshotHelper: HvfMediaImportTestSupport.helper) else { return XCTFail("live import failed") }
        let disk = try XCTUnwrap(config.diskPath)
        let esp = try HvfWindowsBootSeed.readESP(diskPath: disk)
        let vars = config.bundlePath + "/metadata/hvf-vars.fd"
        let data = try Data(contentsOf: URL(fileURLWithPath: vars))
        XCTAssertEqual(data.count, 64 * 1024 * 1024)
        XCTAssertNotNil(data.range(of: esp.partitionGUID))
        XCTAssertNil(data.range(of: HvfWindowsBootSeed.sentinelGUID))
        XCTAssertTrue(HvfWindowsBootSeed.varstoreContainsLiveVariable(data, name: "Boot0000"))
        try JSONSerialization.data(withJSONObject: ["disk": disk, "vars": vars, "boot_entry": true])
            .write(to: URL(fileURLWithPath: output))
    }

    private func checkImport(hasGPT: Bool, suppliedVars: Bool) throws {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try fm.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: root) }
        let source = root.appendingPathComponent("source.raw")
        let guid = Data((1...16).map(UInt8.init))
        let bytes = hasGPT ? HvfWindowsBootSeedTests.makeSyntheticGptDisk(
            espGUID: guid, firstLBA: 34, lastLBA: 40) : Data(repeating: 0x42, count: 4096)
        try bytes.write(to: source)
        let date = Date(timeIntervalSince1970: 1_600_000_000)
        try fm.setAttributes([.modificationDate: date], ofItemAtPath: source.path)
        let seed = try HvfWindowsBootSeed.bundledSeed()
        let vars = root.appendingPathComponent("supplied.fd")
        // Deliberately not a seed: supplied vars must not be patched or provisioned.
        var supplied = seed
        supplied[12345] ^= 0x5a
        if suppliedVars { try supplied.write(to: vars) }
        XCTAssertNil(VMLibrary.windowsHVFImportError(targetDiskPath: source.path,
            varsPath: suppliedVars ? vars.path : ""))
        guard case .created(let config)? = VMLibrary.createWindowsHVF(name: "Import",
            targetDiskPath: source.path, varsPath: suppliedVars ? vars.path : "",
            libraryRoot: root.appendingPathComponent("library"), persist: false,
            snapshotHelper: HvfMediaImportTestSupport.helper) else { return XCTFail("expected imported VM") }
        let result = try Data(contentsOf: URL(fileURLWithPath: config.bundlePath + "/metadata/hvf-vars.fd"))
        XCTAssertEqual(result.count, 64 * 1024 * 1024)
        XCTAssertEqual(result.prefix(64), (suppliedVars ? supplied : seed).prefix(64))
        if suppliedVars {
            XCTAssertEqual(result, supplied)
        } else if hasGPT {
            XCTAssertNotNil(result.range(of: guid))
            XCTAssertNil(result.range(of: HvfWindowsBootSeed.sentinelGUID))
            XCTAssertTrue(HvfWindowsBootSeed.varstoreContainsLiveVariable(result, name: "Boot0000"))
        } else {
            XCTAssertEqual(result, seed)
        }
        XCTAssertEqual(HvfWindowsImportNotice.read(bundlePath: config.bundlePath),
            !suppliedVars && !hasGPT ? HvfWindowsImportNotice.noESP : nil)
        XCTAssertEqual(try Data(contentsOf: source), bytes)
        XCTAssertEqual(try fm.attributesOfItem(atPath: source.path)[.modificationDate] as? Date, date)
    }

    func testFirstRunImportAlsoAcceptsOmittedVars() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let disk = root.appendingPathComponent("disk.raw")
        try Data(repeating: 0, count: 4096).write(to: disk)
        let inputs = FirstRunImport.Inputs(displayName: "Imported", diskPath: disk.path,
            vtpmStateDir: nil, memMiB: 4096, cpuCount: 4)
        XCTAssertNil(FirstRunImport.validate(inputs))
        let config = try FirstRunImport.register(inputs, slug: "imported", libraryRoot: root.appendingPathComponent("library"))
        XCTAssertEqual(HvfWindowsImportNotice.read(bundlePath: config.bundlePath), HvfWindowsImportNotice.noESP)
    }
}
