import CryptoKit
import Foundation
import Security

enum T17TrustDiagnostic {
    static func detail() -> String {
        let metadata = capture()
        let encoded = try? JSONSerialization.data(withJSONObject: metadata, options: [.sortedKeys])
        let identity = encoded.flatMap { String(data: $0, encoding: .utf8) } ?? "unavailable"
        return "macOS Accessibility permission is not granted; caller_identity=" + identity
    }

    static func pathDigest(_ path: String) -> String {
        SHA256.hash(data: Data(path.utf8)).map { String(format: "%02x", $0) }.joined()
    }

    static func signingFields(_ info: [String: Any]) -> [String: String] {
        var fields = ["code_identifier": "unavailable", "code_cdhash": "unavailable"]
        if let identifier = info[kSecCodeInfoIdentifier as String] as? String {
            fields["code_identifier"] = String(identifier.prefix(256))
        }
        if let digest = info[kSecCodeInfoUnique as String] as? Data, !digest.isEmpty {
            fields["code_cdhash"] = digest.prefix(64).map { String(format: "%02x", $0) }.joined()
        }
        return fields
    }

    static func capture() -> [String: String] {
        var fields = [
            "schema": "t17.caller-identity.v1", "pid": String(ProcessInfo.processInfo.processIdentifier),
            "ppid": String(getppid()), "bundle_id": String((Bundle.main.bundleIdentifier ?? "unavailable").prefix(256)),
            "bundle_path_sha256": pathDigest(Bundle.main.bundleURL.resolvingSymlinksInPath().path),
            "executable_name": String((Bundle.main.executableURL?.lastPathComponent ?? "unavailable").prefix(256)),
            "scope": "on-disk-code-metadata-not-signature-validation-or-tcc-attribution"
        ]
        var caller: SecCode?
        let callerStatus = SecCodeCopySelf([], &caller)
        fields["caller_status"] = String(callerStatus)
        guard callerStatus == errSecSuccess, let caller else { return fields }
        var code: SecStaticCode?
        let staticStatus = SecCodeCopyStaticCode(caller, [], &code)
        fields["static_code_status"] = String(staticStatus)
        guard staticStatus == errSecSuccess, let code else { return fields }
        var information: CFDictionary?
        let signingStatus = SecCodeCopySigningInformation(code, [], &information)
        fields["signing_status"] = String(signingStatus)
        guard signingStatus == errSecSuccess else { return fields }
        fields.merge(signingFields(information as? [String: Any] ?? [:])) { _, value in value }
        return fields
    }
}
