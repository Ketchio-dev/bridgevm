import CryptoKit
import Foundation

struct HvfSecureBootPolicy: Codable, Equatable {
    struct Source: Codable, Equatable {
        var repository: String
        var tag: String
        var commit: String
        var asset: String
        var assetSha256: String
        var license: String
    }

    struct Firmware: Codable, Equatable {
        var fileName: String
        var sha256: String
        var edk2Commit: String
    }

    struct Variable: Codable, Equatable {
        var name: String
        var vendorGuid: String
        var attributes: UInt32
        var sha256: String
        var payloadBase64: String
    }

    var schemaVersion: Int
    var policy: String
    var source: Source
    var firmware: Firmware
    var variables: [Variable]
}

struct HvfSecureBootProvisioningReceipt: Codable, Equatable {
    struct Variable: Codable, Equatable {
        var name: String
        var vendorGuid: String
        var attributes: UInt32
        var payloadSha256: String
    }

    var schemaVersion: Int
    var policy: String
    var sourceTag: String
    var sourceCommit: String
    var sourceAssetSha256: String
    var firmwareFileName: String
    var firmwareSha256: String
    var firmwareEdk2Commit: String
    var provisionedAt: String
    var variables: [Variable]
}

enum HvfSecureBootProvisioner {
    struct Result: Equatable {
        var varStore: Data
        var receipt: HvfSecureBootProvisioningReceipt
    }

    struct StoredVariable: Equatable {
        var offset: Int
        var state: UInt8
        var attributes: UInt32
        var name: String
        var guid: Data
        var data: Data
    }

    struct DecodedVariable: Equatable {
        var manifest: HvfSecureBootPolicy.Variable
        var guid: Data
        var payload: Data
    }

    static let authenticatedWriteAttributes: UInt32 = 0x27
    static let imageSecurityDatabaseGUID = Data([
        0xcb, 0xb2, 0x19, 0xd7, 0x3a, 0x3d, 0x96, 0x45,
        0xa3, 0xbc, 0xda, 0xd0, 0x0e, 0x67, 0x65, 0x6f,
    ])

    private static let expectedPolicy = "microsoft-windows-transition-2011-2023"
    private static let expectedSourceTag = "v1.6.5"
    private static let expectedSourceAsset = "edk2-aarch64-secureboot-binaries.zip"
    private static let expectedSourceCommit = "798cdc513e0c192fe90e99637105748ed3bb4ca5"
    private static let expectedAssetSha256 =
        "8c87c63e8ba0385d17238e8feb3b87de25007bec8e43251246bccbf18007af20"
    private static let expectedFirmwareSha256 =
        "b1dc201b1382476ca8c8dcbf8c09abc7ae7429c8437e35bffd54bb9b228b750b"
    private static let expectedEdk2Commit =
        "b03a21a63e3bd001f52c527e5a57feddb53a690b"
    private static let expectedPayloadHashes = [
        "dbx": "329f9ec34a8ae3c9e7eddaeba82a84f598c44853790394314dd88b563c667e1a",
        "db": "584ff437815864a48a2e4c1cc13af8bc19471b140c8085e9de7c738354a91fdc",
        "KEK": "cc3a5dbc7b3aec3b60c0da33510bf93f402479bbf445dc360e6111afa70c6342",
        "PK": "485aca0cb5f875572c905e6f19ec0a249cf438b005a3e27257ac4bd3f56777bd",
    ]

    static func bundledPolicy() throws -> HvfSecureBootPolicy {
        guard let url = Bundle.module.url(
            forResource: "secureboot-microsoft-windows-transition-aarch64-v1.6.5",
            withExtension: "json") else {
            throw HvfWindowsBootSeed.SeedError.secureBootManifestMissing
        }
        do {
            let policy = try JSONDecoder().decode(
                HvfSecureBootPolicy.self, from: Data(contentsOf: url))
            _ = try decodedVariables(policy)
            return policy
        } catch let error as HvfWindowsBootSeed.SeedError {
            throw error
        } catch {
            throw HvfWindowsBootSeed.SeedError.secureBootManifestInvalid(
                "JSON decode: \(error.localizedDescription)")
        }
    }

    static func provision(
        varStore original: Data,
        policy: HvfSecureBootPolicy,
        provisionedAt: Date = Date()
    ) throws -> Result {
        let expected = try decodedVariables(policy)
        let records = try storedVariables(in: original)
        var exactCount = 0
        var missing: [DecodedVariable] = []

        for variable in expected {
            let sameName = records.filter { $0.state == 0x3f && $0.name == variable.manifest.name }
            if sameName.isEmpty {
                missing.append(variable)
                continue
            }
            guard sameName.count == 1 else {
                throw HvfWindowsBootSeed.SeedError.secureBootConflict(
                    "\(variable.manifest.name) 변수가 중복되어 있습니다.")
            }
            let record = sameName[0]
            guard record.guid == variable.guid,
                  record.attributes == variable.manifest.attributes,
                  record.data == variable.payload else {
                throw HvfWindowsBootSeed.SeedError.secureBootConflict(
                    "\(variable.manifest.name) 값이 고정 정책과 다릅니다.")
            }
            exactCount += 1
        }

        if exactCount > 0 && !missing.isEmpty {
            throw HvfWindowsBootSeed.SeedError.secureBootConflict(
                "PK/KEK/db/dbx가 일부만 존재합니다; 명시적인 마이그레이션이 필요합니다.")
        }

        var provisioned = original
        if exactCount == 0 {
            guard let layout = varStoreLayout(provisioned) else {
                throw HvfWindowsBootSeed.SeedError.varstoreUnreadable
            }
            var offset = layout.firstFreeOffset
            for variable in expected {
                let record = HvfWindowsBootSeed.authVariable(
                    name: variable.manifest.name,
                    guid: variable.guid,
                    data: variable.payload,
                    attributes: variable.manifest.attributes)
                guard offset + record.count <= layout.endOffset else {
                    throw HvfWindowsBootSeed.SeedError.varstoreFull
                }
                provisioned.replaceSubrange(offset..<offset + record.count, with: record)
                offset += record.count
            }
        }

        let verified = try storedVariables(in: provisioned)
        for variable in expected {
            let matches = verified.filter {
                $0.state == 0x3f && $0.name == variable.manifest.name &&
                $0.guid == variable.guid &&
                $0.attributes == variable.manifest.attributes &&
                $0.data == variable.payload
            }
            guard matches.count == 1 else {
                throw HvfWindowsBootSeed.SeedError.secureBootManifestInvalid(
                    "\(variable.manifest.name) 기록 후 검증 실패")
            }
        }

        return Result(
            varStore: provisioned,
            receipt: receipt(for: policy, provisionedAt: provisionedAt))
    }

    static func decodedVariables(_ policy: HvfSecureBootPolicy) throws -> [DecodedVariable] {
        guard policy.schemaVersion == 1,
              policy.policy == expectedPolicy,
              policy.source.repository == "https://github.com/microsoft/secureboot_objects",
              policy.source.tag == expectedSourceTag,
              policy.source.asset == expectedSourceAsset,
              policy.source.commit == expectedSourceCommit,
              policy.source.assetSha256 == expectedAssetSha256,
              policy.source.license == "BSD-2-Clause-Patent",
              policy.firmware.fileName == "edk2-aarch64-secure-code.fd",
              policy.firmware.sha256 == expectedFirmwareSha256,
              policy.firmware.edk2Commit == expectedEdk2Commit else {
            throw HvfWindowsBootSeed.SeedError.secureBootManifestInvalid(
                "정책 또는 공급망 provenance가 고정값과 다릅니다.")
        }

        let requiredOrder = ["dbx", "db", "KEK", "PK"]
        guard policy.variables.map(\.name) == requiredOrder else {
            throw HvfWindowsBootSeed.SeedError.secureBootManifestInvalid(
                "키는 dbx, db, KEK, PK 순서여야 하며 PK를 마지막에 기록해야 합니다.")
        }

        return try policy.variables.map { variable in
            let expectedGuid = ["PK", "KEK"].contains(variable.name)
                ? HvfWindowsBootSeed.globalVariableGUID
                : imageSecurityDatabaseGUID
            let expectedGuidString = ["PK", "KEK"].contains(variable.name)
                ? "8be4df61-93ca-11d2-aa0d-00e098032b8c"
                : "d719b2cb-3d3a-4596-a3bc-dad00e67656f"
            guard variable.vendorGuid.lowercased() == expectedGuidString,
                  variable.attributes == authenticatedWriteAttributes,
                  variable.sha256 == expectedPayloadHashes[variable.name],
                  let payload = Data(base64Encoded: variable.payloadBase64),
                  sha256(payload) == variable.sha256 else {
                throw HvfWindowsBootSeed.SeedError.secureBootManifestInvalid(
                    "\(variable.name) GUID/속성/해시 검증 실패")
            }
            try validateSignatureLists(payload, name: variable.name)
            return DecodedVariable(manifest: variable, guid: expectedGuid, payload: payload)
        }
    }

    static func validateSignatureLists(_ payload: Data, name: String) throws {
        var offset = 0
        var listCount = 0
        while offset < payload.count {
            guard payload.count - offset >= 28 else {
                throw malformedESL(name, "잘린 EFI_SIGNATURE_LIST 헤더")
            }
            let listSize = Int(readUInt32(payload, at: offset + 16))
            let headerSize = Int(readUInt32(payload, at: offset + 20))
            let signatureSize = Int(readUInt32(payload, at: offset + 24))
            guard listSize >= 28 + headerSize,
                  signatureSize >= 16,
                  offset + listSize <= payload.count else {
                throw malformedESL(name, "목록 크기가 범위를 벗어남")
            }
            let signaturesSize = listSize - 28 - headerSize
            guard signaturesSize > 0, signaturesSize % signatureSize == 0 else {
                throw malformedESL(name, "서명 항목 크기가 일치하지 않음")
            }
            offset += listSize
            listCount += 1
        }
        guard offset == payload.count, listCount > 0 else {
            throw malformedESL(name, "빈 목록 또는 후행 데이터")
        }
    }

    static func storedVariables(in store: Data) throws -> [StoredVariable] {
        guard let layout = varStoreLayout(store) else {
            throw HvfWindowsBootSeed.SeedError.varstoreUnreadable
        }
        var result: [StoredVariable] = []
        var offset = layout.variableStart
        while offset + 60 <= layout.endOffset {
            if readUInt16(store, at: offset) != 0x55aa { break }
            let state = store[offset + 2]
            let attributes = readUInt32(store, at: offset + 4)
            let nameSize = Int(readUInt32(store, at: offset + 36))
            let dataSize = Int(readUInt32(store, at: offset + 40))
            let nameStart = offset + 60
            let nameEnd = nameStart + nameSize
            let dataEnd = nameEnd + dataSize
            guard nameSize >= 2, nameSize % 2 == 0, dataEnd <= layout.endOffset else {
                throw HvfWindowsBootSeed.SeedError.varstoreUnreadable
            }
            let guid = store.subdata(in: offset + 44..<offset + 60)
            let nameBytes = store.subdata(in: nameStart..<nameEnd)
            var units: [UInt16] = []
            var index = 0
            while index + 1 < nameBytes.count {
                let unit = readUInt16(nameBytes, at: index)
                if unit == 0 { break }
                units.append(unit)
                index += 2
            }
            result.append(StoredVariable(
                offset: offset,
                state: state,
                attributes: attributes,
                name: String(decoding: units, as: UTF16.self),
                guid: guid,
                data: store.subdata(in: nameEnd..<dataEnd)))
            offset += aligned4(60 + nameSize + dataSize)
        }
        return result
    }

    private struct VarStoreLayout {
        var variableStart: Int
        var firstFreeOffset: Int
        var endOffset: Int
    }

    private static func varStoreLayout(_ store: Data) -> VarStoreLayout? {
        guard store.count > 0x80,
              store.subdata(in: 0x28..<0x2b) == Data("_FV".utf8) else { return nil }
        let base = Int(readUInt16(store, at: 0x30))
        guard base + 28 <= store.count,
              store.subdata(in: base..<base + 16) == HvfWindowsBootSeed.authVarStoreGUID else {
            return nil
        }
        let declaredSize = Int(readUInt32(store, at: base + 16))
        let end = min(base + declaredSize, store.count)
        let variableStart = base + 28  // VARIABLE_STORE_HEADER is 28 bytes in EDK2.
        var offset = variableStart
        while offset + 60 <= end, readUInt16(store, at: offset) == 0x55aa {
            let nameSize = Int(readUInt32(store, at: offset + 36))
            let dataSize = Int(readUInt32(store, at: offset + 40))
            let total = aligned4(60 + nameSize + dataSize)
            guard nameSize >= 2, nameSize % 2 == 0,
                  total >= 60, offset + total <= end else { return nil }
            offset += total
        }
        return VarStoreLayout(
            variableStart: variableStart,
            firstFreeOffset: offset,
            endOffset: end)
    }

    private static func receipt(
        for policy: HvfSecureBootPolicy,
        provisionedAt: Date
    ) -> HvfSecureBootProvisioningReceipt {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return HvfSecureBootProvisioningReceipt(
            schemaVersion: 1,
            policy: policy.policy,
            sourceTag: policy.source.tag,
            sourceCommit: policy.source.commit,
            sourceAssetSha256: policy.source.assetSha256,
            firmwareFileName: policy.firmware.fileName,
            firmwareSha256: policy.firmware.sha256,
            firmwareEdk2Commit: policy.firmware.edk2Commit,
            provisionedAt: formatter.string(from: provisionedAt),
            variables: policy.variables.map {
                .init(
                    name: $0.name,
                    vendorGuid: $0.vendorGuid,
                    attributes: $0.attributes,
                    payloadSha256: $0.sha256)
            })
    }

    private static func sha256(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

    private static func malformedESL(
        _ name: String,
        _ detail: String
    ) -> HvfWindowsBootSeed.SeedError {
        .secureBootManifestInvalid("\(name) EFI_SIGNATURE_LIST: \(detail)")
    }

    private static func aligned4(_ value: Int) -> Int { (value + 3) & ~3 }

    private static func readUInt16(_ data: Data, at offset: Int) -> UInt16 {
        UInt16(data[offset]) | (UInt16(data[offset + 1]) << 8)
    }

    private static func readUInt32(_ data: Data, at offset: Int) -> UInt32 {
        (0..<4).reduce(UInt32(0)) {
            $0 | (UInt32(data[offset + $1]) << (8 * UInt32($1)))
        }
    }
}
