import Foundation

/// Seeds a "Windows Boot Manager" UEFI boot variable into a freshly-installed
/// disk's EDK2 varstore so its FIRST boot reaches Windows.
///
/// Why this is needed: the ArmVirtQemu firmware this engine boots does not
/// auto-create a boot option for the NVMe-backed ESP, and WinPE's bcdboot
/// writes the on-disk BCD but leaves no NVRAM boot entry. A cleanly installed
/// disk therefore drops to the UEFI shell. Windows itself only registers its
/// NVRAM entry on its first successful launch — a chicken-and-egg the seed
/// breaks. The device path is partition-signature based (HD(GPT-GUID) →
/// \EFI\Microsoft\Boot\bootmgfw.efi), so it needs only the freshly-assigned
/// ESP partition GUID read from the target's GPT; after the first seeded boot
/// Windows maintains its own entry.
enum HvfWindowsBootSeed {
    struct EspInfo: Equatable {
        var partitionGUID: Data   // 16-byte on-disk GPT unique partition GUID
        var firstLBA: UInt64
        var lastLBA: UInt64
        var partitionNumber: UInt32
    }

    enum SeedError: Error, CustomStringConvertible {
        case gptUnreadable
        case espNotFound
        case varstoreUnreadable
        case varstoreFull
        case secureBootManifestMissing
        case secureBootManifestInvalid(String)
        case secureBootConflict(String)

        var description: String {
            switch self {
            case .gptUnreadable: return "설치 디스크의 GPT를 읽을 수 없습니다."
            case .espNotFound: return "설치 디스크에서 EFI 시스템 파티션을 찾을 수 없습니다."
            case .varstoreUnreadable: return "UEFI vars 저장소 형식을 해석할 수 없습니다."
            case .varstoreFull: return "UEFI vars 저장소에 부팅 항목을 추가할 공간이 없습니다."
            case .secureBootManifestMissing:
                return "번들된 Secure Boot 정책을 찾을 수 없습니다."
            case .secureBootManifestInvalid(let detail):
                return "Secure Boot 정책 검증에 실패했습니다: \(detail)"
            case .secureBootConflict(let detail):
                return "기존 Secure Boot 키를 안전하게 보존하기 위해 프로비저닝을 중단했습니다: \(detail)"
            }
        }
    }

    // EFI System Partition type GUID c12a7328-f81f-11d2-ba4b-00a0c93ec93b,
    // in on-disk mixed-endian byte order.
    static let espTypeGUID = Data([
        0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11,
        0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b,
    ])
    // EFI_GLOBAL_VARIABLE 8be4df61-93ca-11d2-aa0d-00e098032b8c, on-disk order.
    static let globalVariableGUID = Data([
        0x61, 0xdf, 0xe4, 0x8b, 0xca, 0x93, 0xd2, 0x11,
        0xaa, 0x0d, 0x00, 0xe0, 0x98, 0x03, 0x2b, 0x8c,
    ])
    // EDK2 authenticated variable store signature aaf32c78-947b-439a-a180-2e144ec37792.
    static let authVarStoreGUID = Data([
        0x78, 0x2c, 0xf3, 0xaa, 0x7b, 0x94, 0x9a, 0x43,
        0xa1, 0x80, 0x2e, 0x14, 0x4e, 0xc3, 0x77, 0x92,
    ])

    static let bootmgfwPath = #"\EFI\Microsoft\Boot\bootmgfw.efi"#

    // MARK: - GPT

    /// Read the ESP entry from a GPT-formatted raw disk (LBA size 512).
    static func readESP(diskPath: String) throws -> EspInfo {
        guard let handle = FileHandle(forReadingAtPath: diskPath) else {
            throw SeedError.gptUnreadable
        }
        defer { try? handle.close() }
        try handle.seek(toOffset: 512)
        guard let header = try handle.read(upToCount: 512), header.count == 512,
              header.prefix(8) == Data("EFI PART".utf8) else {
            throw SeedError.gptUnreadable
        }
        let partLBA = header.readUInt64(at: 72)
        let count = header.readUInt32(at: 80)
        let entrySize = header.readUInt32(at: 84)
        guard entrySize >= 128, count > 0, count < 1024 else { throw SeedError.gptUnreadable }
        try handle.seek(toOffset: partLBA * 512)
        guard let table = try handle.read(upToCount: Int(count) * Int(entrySize)),
              table.count == Int(count) * Int(entrySize) else {
            throw SeedError.gptUnreadable
        }
        for i in 0..<Int(count) {
            let base = i * Int(entrySize)
            let typeGUID = table.subdata(in: base..<base + 16)
            guard typeGUID == espTypeGUID else { continue }
            let uniqueGUID = table.subdata(in: base + 16..<base + 32)
            let first = table.readUInt64(at: base + 32)
            let last = table.readUInt64(at: base + 40)
            return EspInfo(partitionGUID: uniqueGUID, firstLBA: first, lastLBA: last,
                           partitionNumber: UInt32(i + 1))
        }
        throw SeedError.espNotFound
    }

    // MARK: - Device path + load option

    /// HD() device-path node selecting the ESP by GPT partition signature.
    static func hardDriveNode(_ esp: EspInfo) -> Data {
        var node = Data()
        node.append(0x04)                                   // Type: MEDIA_DEVICE_PATH
        node.append(0x01)                                   // SubType: HARDDRIVE
        node.appendUInt16(42)                               // Length
        node.appendUInt32(esp.partitionNumber)              // PartitionNumber
        node.appendUInt64(esp.firstLBA)                     // PartitionStart
        node.appendUInt64(esp.lastLBA - esp.firstLBA + 1)   // PartitionSize
        node.append(esp.partitionGUID)                      // Signature (16B GPT GUID)
        node.append(0x02)                                   // MBRType: GPT
        node.append(0x02)                                   // SignatureType: GUID
        return node
    }

    /// FilePath() node for \EFI\Microsoft\Boot\bootmgfw.efi.
    static func filePathNode(_ path: String = bootmgfwPath) -> Data {
        var utf16 = Data()
        for scalar in path.utf16 { utf16.appendUInt16(scalar) }
        utf16.appendUInt16(0) // null terminator
        var node = Data()
        node.append(0x04)                       // MEDIA_DEVICE_PATH
        node.append(0x04)                       // FILEPATH
        node.appendUInt16(UInt16(4 + utf16.count))
        node.append(utf16)
        return node
    }

    static let endNode = Data([0x7f, 0xff, 0x04, 0x00])

    /// Full EFI_LOAD_OPTION for "Windows Boot Manager".
    static func loadOption(_ esp: EspInfo, description: String = "Windows Boot Manager") -> Data {
        let devicePath = hardDriveNode(esp) + filePathNode() + endNode
        var descriptionUTF16 = Data()
        for scalar in description.utf16 { descriptionUTF16.appendUInt16(scalar) }
        descriptionUTF16.appendUInt16(0)
        var option = Data()
        option.appendUInt32(0x0000_0001)                    // LOAD_OPTION_ACTIVE
        option.appendUInt16(UInt16(devicePath.count))       // FilePathListLength
        option.append(descriptionUTF16)
        option.append(devicePath)
        return option
    }

    // MARK: - Varstore injection

    private static let varAdded: UInt8 = 0x3f
    private static let attrNvBsRt: UInt32 = 0x7

    /// Returns true if any authenticated variable named `name` in the global
    /// namespace is already present and live.
    static func varstoreContainsLiveVariable(_ store: Data, name: String) -> Bool {
        walkVariables(store) { state, varName, _ in
            state == varAdded && varName == name
        }
    }

    /// Inject Boot0000 (Windows Boot Manager) + BootOrder=[0x0000] into an
    /// EDK2 authenticated varstore image. Idempotent: if a live Windows Boot
    /// Manager entry already exists, the image is returned unchanged.
    static func seed(varStore original: Data, esp: EspInfo) throws -> Data {
        guard let layout = varStoreLayout(original) else { throw SeedError.varstoreUnreadable }
        if varstoreContainsLiveVariable(original, name: "Boot0000") ||
           anyWindowsBootManagerPresent(original) {
            return original
        }
        var store = original
        var offset = layout.firstFreeOffset

        let bootEntry = authVariable(
            name: "Boot0000",
            guid: globalVariableGUID,
            data: loadOption(esp))
        let bootOrder = authVariable(
            name: "BootOrder",
            guid: globalVariableGUID,
            data: Data([0x00, 0x00]))

        for record in [bootEntry, bootOrder] {
            guard offset + record.count <= layout.endOffset else { throw SeedError.varstoreFull }
            store.replaceSubrange(offset..<offset + record.count, with: record)
            offset += record.count
        }
        return store
    }

    /// 16-byte sentinel GPT GUID embedded in the bundled seed varstore's
    /// Windows Boot Manager device path; replaced with the freshly-installed
    /// ESP GUID at seed time.
    static let sentinelGUID = Data("BRIDGEVMESPSEED0".utf8)

    /// Seed `varsPath` so the freshly-installed disk boots Windows on first
    /// power-on. Prefers the bundled proven seed varstore (copy + patch the
    /// ESP GUID); falls back to injecting a boot entry into the store already
    /// at `varsPath` when no bundled seed is available (unit tests).
    @discardableResult
    static func seedFile(
        varsPath: String,
        diskPath: String,
        provisionedAt: Date = Date()
    ) throws -> HvfSecureBootProvisioningReceipt {
        let esp = try readESP(diskPath: diskPath)
        let policy = try HvfSecureBootProvisioner.bundledPolicy()
        let bootSeeded: Data
        if let seed = try? bundledSeed() {
            var patched = seed
            guard replaceAll(&patched, find: sentinelGUID, with: esp.partitionGUID) > 0 else {
                throw SeedError.varstoreUnreadable
            }
            bootSeeded = patched
        } else {
            guard let original = FileManager.default.contents(atPath: varsPath) else {
                throw SeedError.varstoreUnreadable
            }
            let seeded = try seed(varStore: original, esp: esp)
            guard seeded.count == original.count else { throw SeedError.varstoreFull }
            bootSeeded = seeded
        }
        let provisioned = try HvfSecureBootProvisioner.provision(
            varStore: bootSeeded,
            policy: policy,
            provisionedAt: provisionedAt)
        guard provisioned.varStore.count == bootSeeded.count else { throw SeedError.varstoreFull }
        try provisioned.varStore.write(
            to: URL(fileURLWithPath: varsPath), options: [.atomic])
        return provisioned.receipt
    }

    /// Locate and gunzip the bundled proven seed varstore.
    static func bundledSeed() throws -> Data {
        guard let url = Bundle.module.url(
            forResource: "windows-boot-seed-vars", withExtension: "fd.gz") else {
            throw SeedError.varstoreUnreadable
        }
        let tmp = FileManager.default.temporaryDirectory
            .appendingPathComponent("bv-seed-\(UUID().uuidString).fd")
        defer { try? FileManager.default.removeItem(at: tmp) }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/gzip")
        process.arguments = ["-dc", url.path]
        let out = try FileHandle(forWritingTo: {
            FileManager.default.createFile(atPath: tmp.path, contents: nil)
            return tmp
        }())
        process.standardOutput = out
        try process.run()
        process.waitUntilExit()
        try? out.close()
        guard process.terminationStatus == 0,
              let data = FileManager.default.contents(atPath: tmp.path), !data.isEmpty else {
            throw SeedError.varstoreUnreadable
        }
        return data
    }

    @discardableResult
    static func replaceAll(_ data: inout Data, find: Data, with replacement: Data) -> Int {
        guard find.count == replacement.count, !find.isEmpty else { return 0 }
        var count = 0
        var searchStart = data.startIndex
        while let range = data.range(of: find, in: searchStart..<data.endIndex) {
            data.replaceSubrange(range, with: replacement)
            searchStart = range.upperBound
            count += 1
        }
        return count
    }

    // MARK: internals

    private static func anyWindowsBootManagerPresent(_ store: Data) -> Bool {
        var needle = Data()
        for scalar in "Windows Boot Manager".utf16 { needle.appendUInt16(scalar) }
        return store.range(of: needle) != nil
    }

    private struct VarStoreLayout { var firstFreeOffset: Int; var endOffset: Int }

    private static func varStoreLayout(_ store: Data) -> VarStoreLayout? {
        guard store.count > 0x80, store.subdata(in: 0x28..<0x2b) == Data("_FV".utf8) else {
            return nil
        }
        let headerLength = Int(store.readUInt16(at: 0x30))
        let vsBase = headerLength
        guard vsBase + 28 < store.count,
              store.subdata(in: vsBase..<vsBase + 16) == authVarStoreGUID else { return nil }
        let vsSize = Int(store.readUInt32(at: vsBase + 16))
        let dataStart = vsBase + 28  // VARIABLE_STORE_HEADER is 28 bytes in EDK2.
        let end = min(vsBase + vsSize, store.count)
        var offset = dataStart
        // Walk existing (possibly zero) variables to the first free slot.
        while offset + 60 <= end {
            let startId = store.readUInt16(at: offset)
            if startId != 0x55AA { break }
            let nameSize = Int(store.readUInt32(at: offset + 36))
            let dataSize = Int(store.readUInt32(at: offset + 40))
            var total = 60 + nameSize + dataSize
            total = (total + 3) & ~3
            offset += total
        }
        _ = dataStart
        return VarStoreLayout(firstFreeOffset: offset, endOffset: end)
    }

    /// Build one AUTHENTICATED_VARIABLE_HEADER record (4-byte aligned).
    static func authVariable(
        name: String,
        guid: Data,
        data: Data,
        attributes: UInt32 = attrNvBsRt
    ) -> Data {
        var nameUTF16 = Data()
        for scalar in name.utf16 { nameUTF16.appendUInt16(scalar) }
        nameUTF16.appendUInt16(0)

        var record = Data()
        record.appendUInt16(0x55AA)          // StartId
        record.append(varAdded)              // State
        record.append(0x00)                  // Reserved
        record.appendUInt32(attributes)      // Attributes
        record.appendUInt64(0)               // MonotonicCount
        record.append(Data(count: 16))       // TimeStamp
        record.appendUInt32(0)               // PubKeyIndex
        record.appendUInt32(UInt32(nameUTF16.count))
        record.appendUInt32(UInt32(data.count))
        record.append(guid)                  // VendorGuid (16B)
        record.append(nameUTF16)
        record.append(data)
        while record.count % 4 != 0 { record.append(0xff) }
        return record
    }

    /// Visit each live variable; `body` may short-circuit by returning true.
    @discardableResult
    private static func walkVariables(
        _ store: Data,
        _ body: (_ state: UInt8, _ name: String, _ dataRange: Range<Int>) -> Bool
    ) -> Bool {
        guard store.count > 0x80, store.subdata(in: 0x28..<0x2b) == Data("_FV".utf8) else {
            return false
        }
        let headerLength = Int(store.readUInt16(at: 0x30))
        let vsBase = headerLength
        guard vsBase + 28 < store.count,
              store.subdata(in: vsBase..<vsBase + 16) == authVarStoreGUID else { return false }
        let vsSize = Int(store.readUInt32(at: vsBase + 16))
        let end = min(vsBase + vsSize, store.count)
        var offset = vsBase + 28  // VARIABLE_STORE_HEADER is 28 bytes in EDK2.
        while offset + 60 <= end {
            let startId = store.readUInt16(at: offset)
            if startId != 0x55AA { break }
            let state = store[store.startIndex + offset + 2]
            let nameSize = Int(store.readUInt32(at: offset + 36))
            let dataSize = Int(store.readUInt32(at: offset + 40))
            let nameStart = offset + 60
            let nameEnd = nameStart + nameSize
            let dataStart = nameEnd
            let dataEnd = dataStart + dataSize
            if nameEnd <= end, dataEnd <= end {
                let nameData = store.subdata(in: nameStart..<nameEnd)
                var codeUnits: [UInt16] = []
                var index = nameData.startIndex
                while index + 1 < nameData.endIndex {
                    let unit = UInt16(nameData[index]) | (UInt16(nameData[index + 1]) << 8)
                    if unit == 0 { break }
                    codeUnits.append(unit)
                    index += 2
                }
                let name = String(decoding: codeUnits, as: UTF16.self)
                if body(state, name, dataStart..<dataEnd) { return true }
            }
            var total = 60 + nameSize + dataSize
            total = (total + 3) & ~3
            offset += total
        }
        return false
    }
}

// MARK: - Little-endian Data helpers

private extension Data {
    func readUInt16(at offset: Int) -> UInt16 {
        let base = startIndex + offset
        return UInt16(self[base]) | (UInt16(self[base + 1]) << 8)
    }
    func readUInt32(at offset: Int) -> UInt32 {
        let base = startIndex + offset
        return (0..<4).reduce(UInt32(0)) { $0 | (UInt32(self[base + $1]) << (8 * $1)) }
    }
    func readUInt64(at offset: Int) -> UInt64 {
        let base = startIndex + offset
        return (0..<8).reduce(UInt64(0)) { $0 | (UInt64(self[base + $1]) << (8 * $1)) }
    }
    mutating func appendUInt16(_ value: UInt16) {
        append(UInt8(value & 0xff)); append(UInt8((value >> 8) & 0xff))
    }
    mutating func appendUInt32(_ value: UInt32) {
        for i in 0..<4 { append(UInt8((value >> (8 * i)) & 0xff)) }
    }
    mutating func appendUInt64(_ value: UInt64) {
        for i in 0..<8 { append(UInt8((value >> (8 * UInt64(i))) & 0xff)) }
    }
}
