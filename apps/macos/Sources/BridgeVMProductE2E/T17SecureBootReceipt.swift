import Foundation

enum T17SecureBootReceipt {
    private struct Policy: Decodable {
        struct Source: Decodable { let tag: String; let commit: String; let assetSha256: String }
        struct Firmware: Decodable { let fileName: String; let sha256: String; let edk2Commit: String }
        struct Variable: Decodable { let name: String; let vendorGuid: String; let attributes: UInt32; let sha256: String }
        let schemaVersion: Int
        let policy: String
        let source: Source
        let firmware: Firmware
        let variables: [Variable]
    }

    private struct Receipt: Decodable {
        struct Variable: Decodable { let name: String; let vendorGuid: String; let attributes: UInt32; let payloadSha256: String }
        let schemaVersion: Int
        let policy: String
        let sourceTag: String
        let sourceCommit: String
        let sourceAssetSha256: String
        let firmwareFileName: String
        let firmwareSha256: String
        let firmwareEdk2Commit: String
        let provisionedAt: String
        let variables: [Variable]
    }

    static func verify(receipt: URL, policy: URL) throws {
        let decoder = JSONDecoder()
        let policy = try decoder.decode(Policy.self, from: bounded(policy))
        let receipt = try decoder.decode(Receipt.self, from: bounded(receipt))
        guard policy.schemaVersion == 1, receipt.schemaVersion == 1,
              receipt.policy == policy.policy,
              receipt.sourceTag == policy.source.tag,
              receipt.sourceCommit == policy.source.commit,
              receipt.sourceAssetSha256 == policy.source.assetSha256,
              receipt.firmwareFileName == policy.firmware.fileName,
              receipt.firmwareSha256 == policy.firmware.sha256,
              receipt.firmwareEdk2Commit == policy.firmware.edk2Commit,
              receipt.provisionedAt.range(
                of: #"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z$"#,
                options: .regularExpression) != nil,
              receipt.variables.count == policy.variables.count,
              zip(receipt.variables, policy.variables).allSatisfy({ actual, expected in
                  actual.name == expected.name && actual.vendorGuid == expected.vendorGuid
                      && actual.attributes == expected.attributes && actual.payloadSha256 == expected.sha256
              }) else {
            throw T17Blocker(code: "installer-failed", detail: "Secure Boot receipt does not match its packaged policy")
        }
    }

    private static func bounded(_ url: URL) throws -> Data {
        let values = try url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey])
        guard values.isRegularFile == true, values.isSymbolicLink != true,
              let size = values.fileSize, size > 0, size <= 1_048_576 else {
            throw T17Blocker(code: "installer-failed", detail: "Secure Boot JSON is missing or unsafe")
        }
        return try Data(contentsOf: url)
    }
}
