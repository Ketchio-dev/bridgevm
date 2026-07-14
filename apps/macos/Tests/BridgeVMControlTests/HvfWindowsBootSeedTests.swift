import XCTest
@testable import BridgeVMControl

final class HvfWindowsBootSeedTests: XCTestCase {

    // MARK: device path + load option

    func testHardDriveNodeMatchesEdk2GptLayout() {
        let esp = HvfWindowsBootSeed.EspInfo(
            partitionGUID: Data((0..<16).map { UInt8($0) }),
            firstLBA: 2048, lastLBA: 534527, partitionNumber: 1)
        let node = HvfWindowsBootSeed.hardDriveNode(esp)
        XCTAssertEqual(node.count, 42)
        XCTAssertEqual(node[0], 0x04)  // MEDIA
        XCTAssertEqual(node[1], 0x01)  // HARDDRIVE
        XCTAssertEqual(Int(node[2]) | (Int(node[3]) << 8), 42)
        // signature GUID occupies bytes 24..<40
        XCTAssertEqual(node.subdata(in: 24..<40), esp.partitionGUID)
        XCTAssertEqual(node[40], 0x02)  // GPT
        XCTAssertEqual(node[41], 0x02)  // GUID sig type
        // start LBA at 8, size at 16
        XCTAssertEqual(node.readLE64(24 - 16), 2048)  // start at offset 8
        XCTAssertEqual(node.readLE64(16), 532480)     // size
    }

    func testLoadOptionCarriesActiveFlagAndBootmgfwPath() {
        let esp = HvfWindowsBootSeed.EspInfo(
            partitionGUID: Data(count: 16), firstLBA: 2048, lastLBA: 534527, partitionNumber: 1)
        let option = HvfWindowsBootSeed.loadOption(esp)
        XCTAssertEqual(option.readLE32(0), 0x1)  // LOAD_OPTION_ACTIVE
        let fpLen = Int(option.readLE16(4))
        // device path = HD(42) + FILE(70) + END(4)
        XCTAssertEqual(fpLen, 42 + 70 + 4)
        // description "Windows Boot Manager" present as UTF-16
        var descNeedle = Data()
        for scalar in "Windows Boot Manager".utf16 {
            descNeedle.append(UInt8(scalar & 0xff)); descNeedle.append(UInt8(scalar >> 8))
        }
        XCTAssertNotNil(option.range(of: descNeedle))
        var pathNeedle = Data()
        for scalar in #"\EFI\Microsoft\Boot\bootmgfw.efi"#.utf16 {
            pathNeedle.append(UInt8(scalar & 0xff)); pathNeedle.append(UInt8(scalar >> 8))
        }
        XCTAssertNotNil(option.range(of: pathNeedle))
    }

    // MARK: varstore injection

    func testSeedInjectsBootEntryAndBootOrderIntoEmptyVarstore() throws {
        let store = Self.makeEmptyAuthVarstore()
        let esp = HvfWindowsBootSeed.EspInfo(
            partitionGUID: Data((0..<16).map { UInt8(0xA0 + $0) }),
            firstLBA: 2048, lastLBA: 534527, partitionNumber: 1)
        XCTAssertFalse(HvfWindowsBootSeed.varstoreContainsLiveVariable(store, name: "Boot0000"))

        let seeded = try HvfWindowsBootSeed.seed(varStore: store, esp: esp)
        XCTAssertEqual(seeded.count, store.count, "seeding must not change the image size")
        XCTAssertTrue(HvfWindowsBootSeed.varstoreContainsLiveVariable(seeded, name: "Boot0000"))
        XCTAssertTrue(HvfWindowsBootSeed.varstoreContainsLiveVariable(seeded, name: "BootOrder"))
        // The ESP GUID must appear inside the injected device path.
        XCTAssertNotNil(seeded.range(of: esp.partitionGUID))
    }

    func testSeedIsIdempotent() throws {
        let store = Self.makeEmptyAuthVarstore()
        let esp = HvfWindowsBootSeed.EspInfo(
            partitionGUID: Data(count: 16), firstLBA: 2048, lastLBA: 534527, partitionNumber: 1)
        let once = try HvfWindowsBootSeed.seed(varStore: store, esp: esp)
        let twice = try HvfWindowsBootSeed.seed(varStore: once, esp: esp)
        XCTAssertEqual(once, twice, "a store that already has the entry is returned unchanged")
    }

    // MARK: GPT read (round-trip through a synthetic disk)

    func testReadESPParsesSyntheticGpt() throws {
        let temp = FileManager.default.temporaryDirectory
            .appendingPathComponent("bootseed-\(UUID().uuidString).raw")
        defer { try? FileManager.default.removeItem(at: temp) }
        let espGUID = Data((0..<16).map { UInt8(0x10 + $0) })
        let disk = Self.makeSyntheticGptDisk(espGUID: espGUID, firstLBA: 2048, lastLBA: 534527)
        try disk.write(to: temp)

        let esp = try HvfWindowsBootSeed.readESP(diskPath: temp.path)
        XCTAssertEqual(esp.partitionGUID, espGUID)
        XCTAssertEqual(esp.firstLBA, 2048)
        XCTAssertEqual(esp.lastLBA, 534527)
        XCTAssertEqual(esp.partitionNumber, 1)
    }

    /// Escape hatch used only during live E2E validation: when a real
    /// installed disk is staged at a well-known path, produce a seeded vars
    /// store from the pristine template so a live boot can verify the exact
    /// Swift-produced image. Skipped in normal CI (no such disk).
    func testSeedRealInstalledDiskWhenStaged() throws {
        let disk = "/tmp/bridgevm-appinstall-e2e-app-target.raw"
        let template = "/opt/homebrew/share/qemu/edk2-arm-vars.fd"
        let out = "/tmp/bridgevm-appinstall-e2e-verify-vars.fd"
        guard FileManager.default.isReadableFile(atPath: disk),
              FileManager.default.isReadableFile(atPath: template) else {
            throw XCTSkip("no staged installed disk; live-only helper")
        }
        try? FileManager.default.removeItem(atPath: out)
        try FileManager.default.copyItem(atPath: template, toPath: out)
        try HvfWindowsBootSeed.seedFile(varsPath: out, diskPath: disk)
        let seeded = try XCTUnwrap(FileManager.default.contents(atPath: out))
        // seedFile prefers the bundled seed and patches the ESP GUID into it,
        // so assert the boot-manager entry and the patched GUID are present.
        var wbm = Data()
        for scalar in "Windows Boot Manager".utf16 {
            wbm.append(UInt8(scalar & 0xff)); wbm.append(UInt8(scalar >> 8))
        }
        XCTAssertNotNil(seeded.range(of: wbm))
        let esp = try HvfWindowsBootSeed.readESP(diskPath: disk)
        XCTAssertNotNil(seeded.range(of: esp.partitionGUID))
        XCTAssertNil(seeded.range(of: HvfWindowsBootSeed.sentinelGUID),
                     "sentinel GUID must be fully replaced")
    }

    func testBundledSeedDecompressesAndCarriesSentinel() throws {
        let seed = try HvfWindowsBootSeed.bundledSeed()
        XCTAssertGreaterThan(seed.count, 1024)
        XCTAssertNotNil(seed.range(of: HvfWindowsBootSeed.sentinelGUID),
                        "bundled seed must carry the patchable sentinel GUID")
        var wbm = Data()
        for scalar in "Windows Boot Manager".utf16 {
            wbm.append(UInt8(scalar & 0xff)); wbm.append(UInt8(scalar >> 8))
        }
        XCTAssertNotNil(seed.range(of: wbm))
    }

    // MARK: fixtures

    /// Minimal FV+auth-varstore image matching the edk2-arm-vars.fd shape this
    /// seed targets: `_FV` signature at 0x28, HeaderLength at 0x30, the auth
    /// varstore GUID at the header offset, then an empty (zeroed) data region.
    private static func makeEmptyAuthVarstore(size: Int = 256 * 1024) -> Data {
        var store = Data(count: size)
        store.replaceSubrange(0x28..<0x2b, with: Data("_FV".utf8))
        // HeaderLength = 72 (0x48) at 0x30 (UInt16 LE)
        store[0x30] = 0x48
        store[0x31] = 0x00
        let vsBase = 0x48
        store.replaceSubrange(vsBase..<vsBase + 16, with: HvfWindowsBootSeed.authVarStoreGUID)
        // vsSize at vsBase+16 (UInt32 LE) — cover the rest of the image.
        let vsSize = UInt32(size - vsBase)
        for i in 0..<4 { store[vsBase + 16 + i] = UInt8((vsSize >> (8 * UInt32(i))) & 0xff) }
        store[vsBase + 20] = 0x5a  // format healthy
        store[vsBase + 21] = 0xfe  // state healthy
        return store
    }

    private static func makeSyntheticGptDisk(espGUID: Data, firstLBA: UInt64, lastLBA: UInt64) -> Data {
        var disk = Data(count: 512 * (34 + 8))  // protective MBR + GPT header + entries + slack
        // GPT header at LBA1
        var header = Data("EFI PART".utf8)
        header.append(Data(count: 512 - header.count))
        // PartitionEntryLBA (offset 72) = 2, NumberOfEntries (80) = 4, SizeOfEntry (84) = 128
        func put64(_ d: inout Data, _ off: Int, _ v: UInt64) {
            for i in 0..<8 { d[off + i] = UInt8((v >> (8 * UInt64(i))) & 0xff) }
        }
        func put32(_ d: inout Data, _ off: Int, _ v: UInt32) {
            for i in 0..<4 { d[off + i] = UInt8((v >> (8 * UInt32(i))) & 0xff) }
        }
        put64(&header, 72, 2)
        put32(&header, 80, 4)
        put32(&header, 84, 128)
        disk.replaceSubrange(512..<1024, with: header)
        // ESP entry at LBA2
        var entry = Data()
        entry.append(HvfWindowsBootSeed.espTypeGUID)  // type GUID
        entry.append(espGUID)                          // unique GUID
        var firstBytes = Data(); for i in 0..<8 { firstBytes.append(UInt8((firstLBA >> (8 * UInt64(i))) & 0xff)) }
        var lastBytes = Data(); for i in 0..<8 { lastBytes.append(UInt8((lastLBA >> (8 * UInt64(i))) & 0xff)) }
        entry.append(firstBytes)
        entry.append(lastBytes)
        entry.append(Data(count: 128 - entry.count))
        disk.replaceSubrange(1024..<1024 + 128, with: entry)
        return disk
    }
}

private extension Data {
    func readLE16(_ off: Int) -> UInt16 {
        UInt16(self[startIndex + off]) | (UInt16(self[startIndex + off + 1]) << 8)
    }
    func readLE32(_ off: Int) -> UInt32 {
        (0..<4).reduce(UInt32(0)) { $0 | (UInt32(self[startIndex + off + $1]) << (8 * $1)) }
    }
    func readLE64(_ off: Int) -> UInt64 {
        (0..<8).reduce(UInt64(0)) { $0 | (UInt64(self[startIndex + off + $1]) << (8 * UInt64($1))) }
    }
}
