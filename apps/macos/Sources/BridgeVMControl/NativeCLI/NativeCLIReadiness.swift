import Foundation

struct NativeCLIReadinessIssue: Codable, Equatable {
    let code: String
    let scope: String
    let summary: String

    init(_ issue: HvfWindowsReadinessIssue) {
        code = issue.code
        scope = issue.scope.rawValue
        summary = issue.summary
    }
}

struct NativeCLIReadiness: Encodable {
    let schema = "bridgevm.app-readiness.v1"
    let libraryPath: String
    let id: String
    let backendKind: String
    let runtimeState = "unobserved"
    let engineChecksPerformed: Bool
    let launchReady: Bool
    let launchBlockers: [NativeCLIReadinessIssue]
    let releaseBlockers: [NativeCLIReadinessIssue]
    let productLimitations: [String]
    let recoveryObservations: [String]

    static func snapshot(rootURL: URL, id: String,
                         repoRoot: URL = HvfEngineSession.defaultRepoRoot()) throws -> Self {
        let config = try NativeLibraryReader.readConfig(rootURL: rootURL, id: id)
        guard config.backendKind == "hvf-engine" else {
            return blocked(config: config, rootURL: rootURL, code: "unsupported-backend",
                           summary: "This readiness query supports only the saved hvf-engine backend.")
        }
        guard config.installPending != true else {
            return blocked(config: config, rootURL: rootURL, code: "installation-pending",
                           summary: "Windows installation must finish before installed-VM launch readiness can be checked.")
        }
        guard let engine = HvfEngineConfig.libraryVM(config, rootURL: rootURL) else {
            throw NativeCLIError.unavailable("Cannot form the saved VM's HVF launch configuration.")
        }
        return Self(config: config, rootURL: rootURL,
                    report: engine.readiness(repoRoot: repoRoot), engineChecksPerformed: true)
    }

    private static func blocked(config: VMConfig, rootURL: URL, code: String, summary: String) -> Self {
        let issue = HvfWindowsReadinessIssue(code: code, scope: .launch, summary: summary)
        let report = HvfWindowsReadinessReport(issues: [issue], productLimitations: [])
        return Self(config: config, rootURL: rootURL, report: report, engineChecksPerformed: false)
    }

    private init(config: VMConfig, rootURL: URL, report: HvfWindowsReadinessReport,
                 engineChecksPerformed: Bool) {
        libraryPath = rootURL.path
        id = config.slug
        backendKind = config.backendKind
        self.engineChecksPerformed = engineChecksPerformed
        launchReady = engineChecksPerformed && report.launchReady
        launchBlockers = report.launchBlockers.map(NativeCLIReadinessIssue.init)
        releaseBlockers = report.releaseBlockers.map(NativeCLIReadinessIssue.init)
        productLimitations = report.productLimitations
        recoveryObservations = NativeLibraryRecord(config: config, rootURL: rootURL).recoveryObservations
    }
}
