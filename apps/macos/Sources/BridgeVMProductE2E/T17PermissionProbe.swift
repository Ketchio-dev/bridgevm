import ApplicationServices
import Foundation

enum T17PermissionProbe {
    static func accepts(_ arguments: [String]) -> Bool {
        arguments == ["--accessibility-diagnostic"]
    }

    static func encode(identity: [String: String], trusted: Bool) throws -> Data {
        let report: [String: Any] = [
            "schema": "t17.accessibility-diagnostic.v1", "observation_only": true,
            "criterion_pass": false, "accessibility_trusted": trusted,
            "caller_identity": identity,
            "scope": "calling-process-only-not-product-e2e-or-tcc-database-attribution"
        ]
        return try JSONSerialization.data(withJSONObject: report, options: [.sortedKeys]) + Data([10])
    }

    static func run(_ arguments: [String]) throws -> Bool {
        guard accepts(arguments) else { return false }
        let report = try encode(identity: T17TrustDiagnostic.capture(), trusted: AXIsProcessTrusted())
        FileHandle.standardOutput.write(report)
        return true
    }
}
