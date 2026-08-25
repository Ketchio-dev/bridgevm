import Foundation
struct HvfWindowsDriverPreflight: Equatable {
    static let provenanceBlocker = "kernel-policy-provenance-unverifiable"
    let blocker: String?
    let signingMode: String
    let testSigningRequired: Bool?

    static func inspect(
        packageDirectory: String,
        now: Date = Date(),
        trustAnchors: [HvfWindowsKernelPolicyVerifier.TrustAnchor] =
            HvfWindowsKernelPolicyVerifier.productionTrustAnchors
    ) -> HvfWindowsDriverPreflight {
        let directory = URL(fileURLWithPath: packageDirectory, isDirectory: true)
        let report = HvfWindowsKernelPolicyVerifier.inspectReport(packageDirectory: directory)
        if report.blocker != nil {
            return .init(blocker: report.blocker, signingMode: report.signingMode,
                         testSigningRequired: report.testSigningRequired)
        }
        let verification = HvfWindowsKernelPolicyVerifier.verify(
            packageDirectory: directory, now: now, trustAnchors: trustAnchors)
        switch verification {
        case .success:
            return .init(blocker: nil, signingMode: report.signingMode,
                         testSigningRequired: report.testSigningRequired)
        case .failure(let failure):
            return .init(blocker: failure.rawValue, signingMode: report.signingMode,
                         testSigningRequired: report.testSigningRequired)
        }
    }

    static func message(for blocker: String) -> String {
        "3D 드라이버 사전 점검 차단 [\(blocker)]: Windows-HVF 3D 주입에는 BridgeVM이 " +
        "검증한 서명 provenance와 kernel-policy 패키지가 필요합니다. 3D 주입을 끄고 다시 생성하세요."
    }

    var userMessage: String? { blocker.map(Self.message) }
}
