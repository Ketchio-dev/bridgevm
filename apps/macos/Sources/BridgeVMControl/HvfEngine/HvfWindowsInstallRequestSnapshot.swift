import CryptoKit
import Foundation

/// The decoded request and digest always describe the exact bytes to be staged.
struct HvfWindowsInstallRequestSnapshot {
    let data: Data
    let request: HvfWindowsInstallRequest
    let sha256: String

    init(data: Data, expectedSHA256: String? = nil) throws {
        guard data.count <= VMLibrary.maximumConfigBytes else {
            throw HvfWindowsInstallFinalizationError.invalidState("Installation request exceeds its byte limit.")
        }
        let request = try JSONDecoder().decode(HvfWindowsInstallRequest.self, from: data)
        let digest = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        if let expectedSHA256, digest != expectedSHA256 {
            throw HvfWindowsInstallFinalizationError.invalidState("Installation request digest differs from journal.")
        }
        self.data = data
        self.request = request
        self.sha256 = digest
    }

    static func load(_ url: URL, expectedSHA256: String? = nil) throws -> Self {
        try Self(data: HvfWindowsInstallDurability.readRegularFile(
            url, maximumBytes: VMLibrary.maximumConfigBytes), expectedSHA256: expectedSHA256)
    }
}
