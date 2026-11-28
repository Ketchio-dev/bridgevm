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

    private let noESP = "This disk has no boot partition, so Windows may not start."

    func testLegacyMissingPartitionNoticeDisplaysInEnglishAfterReopening() throws {
        let bundle = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let metadata = bundle.appendingPathComponent("metadata")
        try FileManager.default.createDirectory(at: metadata, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: bundle) }
        let file = metadata.appendingPathComponent("hvf-import-notice.txt")
        for text in ["이 디스크에는 UEFI 부팅 파티션이 없어 시작되지 않을 수 있습니다.",
                     "This disk has no UEFI boot partition. Windows may fail to start."] {
            let original = Data(text.utf8)
            try original.write(to: file)
            XCTAssertEqual(HvfWindowsImportNotice.read(bundlePath: bundle.path), noESP)
            XCTAssertEqual(try Data(contentsOf: file), original)
        }
    }

    private func notice(bundlePath: String) -> String? {
        try? String(contentsOfFile: bundlePath + "/metadata/hvf-import-notice.txt", encoding: .utf8)
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
            XCTAssertTrue(HvfWindowsBootSeed.varstoreContainsLiveVariable(result, name: "Boot0003"))
        } else {
            XCTAssertEqual(result, seed)
        }
        XCTAssertEqual(notice(bundlePath: config.bundlePath),
            !suppliedVars && !hasGPT ? noESP : nil)
        var state = CreateVMCreationState()
        let presentation = state.complete(.created(config), adopt: { _ in true })
        XCTAssertEqual(presentation.dismiss, suppliedVars || hasGPT)
        XCTAssertEqual(presentation.message, !suppliedVars && !hasGPT ? noESP : "")
        XCTAssertEqual(try Data(contentsOf: source), bytes)
        XCTAssertEqual(try fm.attributesOfItem(atPath: source.path)[.modificationDate] as? Date, date)
    }

    func testGPTWithoutESPImportsWithPlainSeed() throws {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try fm.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: root) }
        let source = root.appendingPathComponent("data.raw")
        var bytes = HvfWindowsBootSeedTests.makeSyntheticGptDisk(
            espGUID: Data(repeating: 1, count: 16), firstLBA: 34, lastLBA: 40)
        bytes.replaceSubrange(1024..<1040, with: Data(count: 16))
        try bytes.write(to: source)
        let before = try fm.attributesOfItem(atPath: source.path)[.modificationDate] as? Date
        guard case .created(let config)? = VMLibrary.createWindowsHVF(name: "Data",
            targetDiskPath: source.path, varsPath: "", libraryRoot: root.appendingPathComponent("library"),
            persist: false, snapshotHelper: HvfMediaImportTestSupport.helper) else { return XCTFail("data disk must import") }
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: config.bundlePath + "/metadata/hvf-vars.fd")),
            try HvfWindowsBootSeed.bundledSeed())
        XCTAssertEqual(notice(bundlePath: config.bundlePath), noESP)
        XCTAssertEqual(try Data(contentsOf: source), bytes)
        XCTAssertEqual(try fm.attributesOfItem(atPath: source.path)[.modificationDate] as? Date, before)
    }

    @MainActor
    func testFirstRunCanOpenSavedImportAfterBootPartitionNotice() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let disk = root.appendingPathComponent("disk.raw")
        try Data(repeating: 0, count: 4096).write(to: disk)
        let inputs = FirstRunImport.Inputs(displayName: "Imported", diskPath: disk.path, varsPath: "",
            vtpmStateDir: nil, memMiB: 4096, cpuCount: 4)
        let library = LibraryModel(rootURL: root.appendingPathComponent("library"), migrateLegacy: false)
        let warning = await library.importExistingHvfVM(inputs, snapshotHelper: HvfMediaImportTestSupport.helper)
        XCTAssertEqual(warning, noESP)
        XCTAssertTrue(library.firstRunImport.savedWithNotice)
        XCTAssertEqual(library.firstRunImport.stage, .complete)
        let saved = try XCTUnwrap(library.firstRunImport.publishedConfig)
        XCTAssertEqual(notice(bundlePath: saved.bundlePath), noESP)
        try FileManager.default.removeItem(at: disk)

        let readback = await library.recoverPublishedFirstRunImport { _, _ in nil }
        XCTAssertTrue(try XCTUnwrap(readback).contains(noESP))
        XCTAssertFalse(library.firstRunImport.savedWithNotice)
        XCTAssertEqual(library.firstRunImport.publishedConfig?.slug, saved.slug)

        let error = await library.recoverPublishedFirstRunImport()
        XCTAssertNil(error)
        XCTAssertEqual(library.selectedID, saved.slug)
        XCTAssertEqual(library.vms.map(\.slug), [saved.slug])
    }
}
