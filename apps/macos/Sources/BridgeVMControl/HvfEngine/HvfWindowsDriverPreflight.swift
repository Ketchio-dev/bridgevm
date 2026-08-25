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
        "3D 드라이버 사전 점검 차단 [\(blocker)]: Windows-HVF 3D 주입에는 검증된 서명 " +
        "provenance가 필요합니다. 검증기는 아직 구현되지 않았습니다. 3D 주입을 끄고 다시 생성하세요."
    }

    var userMessage: String? { blocker.map(Self.message) }
}
