import Foundation
struct HvfWindowsDriverPreflight: Equatable {
    static let provenanceBlocker = "kernel-policy-provenance-unverifiable"
    let blocker: String?
    let signingMode: String
    let testSigningRequired: Bool?

    static func inspect(packageDirectory: String) -> HvfWindowsDriverPreflight {
        let report = URL(fileURLWithPath: packageDirectory)
            .appendingPathComponent("bridgevm-finalization-report.txt")
        guard let text = try? String(contentsOf: report, encoding: .ascii) else {
            return .init(blocker: "signing-report-missing", signingMode: "unknown",
                         testSigningRequired: nil)
        }
        func value(_ key: String) -> String? {
            text.split(whereSeparator: \.isNewline).first { $0.hasPrefix("\(key)=") }
                .map { String($0.dropFirst(key.count + 1)) }
        }
        guard value("finalization_complete") == "true",
              let mode = value("signing_mode"),
              let requiredText = value("test_signing_required"),
              let required = Bool(requiredText) else {
            return .init(blocker: "signing-report-invalid", signingMode: "unknown",
                         testSigningRequired: nil)
        }
        if required || mode == "test" {
            return .init(blocker: "test-signing-blocked-by-secure-boot", signingMode: mode,
                         testSigningRequired: required)
        }
        guard mode == "kernel-policy",
              value("sys_kernel_policy_verified") == "true",
              value("cat_kernel_policy_verified") == "true" else {
            return .init(blocker: "kernel-policy-unverifiable", signingMode: mode,
                         testSigningRequired: required)
        }
        return .init(blocker: provenanceBlocker, signingMode: mode, testSigningRequired: required)
    }

    static func message(for blocker: String) -> String {
        "BridgeVM can’t verify the signing provenance of the Windows 3D driver. Turn off 3D driver injection and create the VM again. [\(blocker)]"
    }

    var userMessage: String? { blocker.map(Self.message) }
}
