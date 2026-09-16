#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation
import Security

enum AppUIDriverValidation {
    static func makeNonce() throws -> String {
        var bytes = [UInt8](repeating: 0, count: 32)
        let status = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        return try nonce(bytes: bytes, providerStatus: status)
    }

    static func nonce(bytes: [UInt8], providerStatus: OSStatus) throws -> String {
        guard providerStatus == errSecSuccess, bytes.count == 32 else {
            throw AppUIDriverFailure.internalFailure
        }
        return bytes.map { String(format: "%02x", $0) }.joined()
    }

    static func hexDigest(_ value: String) -> Bool {
        value.utf8.count == 64 && value.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) }
    }

    static func canonicalPath(_ path: String) -> Bool {
        let components = path.split(separator: "/", omittingEmptySubsequences: false)
        // Foundation standardization rewrites /private/var to /var on macOS.
        // Check lexical form here; descriptor traversal refuses real symlinks.
        return path.hasPrefix("/") && path.utf8.count <= 4096 && !path.contains("\0")
            && components.count > 1 && components.dropFirst().allSatisfy {
                !$0.isEmpty && $0 != "." && $0 != ".."
            }
    }

    static func session(_ value: AppUIDriverSession) throws {
        guard value.schemaVersion == 1, value.kind == "native-app-ui-driver-session",
              hexDigest(value.nonce), value.startedUptime.isFinite, value.startedUptime >= 0,
              value.deadlineUptime.isFinite, value.deadlineUptime - value.startedUptime == 90,
              value.host.pid != value.driver.pid else { throw AppUIDriverFailure.invalidSession }
        try identity(value.host, driver: false)
        try identity(value.driver, driver: true)
        let hostParent = URL(fileURLWithPath: value.host.bundlePath).deletingLastPathComponent()
        let driverParent = URL(fileURLWithPath: value.driver.bundlePath).deletingLastPathComponent()
        guard hostParent == driverParent, hostParent.lastPathComponent == "app-ui-private" else {
            throw AppUIDriverFailure.invalidSession
        }
    }

    static func identity(_ value: AppUIDriverProcessIdentity, driver: Bool) throws {
        let identifier = driver ? AppUIDriverConstants.driverBundleIdentifier : AppUIDriverConstants.hostBundleIdentifier
        let bundleName = driver ? "BridgeVMAppUIDriver.app" : "BridgeVMAppUIHost.app"
        let executable = driver ? "AppUIHostLauncher" : "BridgeVMControl"
        guard value.pid > 1, value.launchDate.isFinite, value.launchDate > 0,
              value.bundleIdentifier == identifier, canonicalPath(value.bundlePath),
              URL(fileURLWithPath: value.bundlePath).lastPathComponent == bundleName,
              value.executablePath == value.bundlePath + "/Contents/MacOS/" + executable,
              hexDigest(value.executableSHA256) else { throw AppUIDriverFailure.identityMismatch }
    }

    static func request(_ value: AppUIDriverRequest, session: AppUIDriverSession,
                        sessionSHA256: String, now: Double) throws {
        guard value.schemaVersion == 1, value.kind == "native-app-ui-driver-request",
              hexDigest(sessionSHA256), value.sessionSHA256 == sessionSHA256,
              value.nonce == session.nonce, (1...AppUIDriverConstants.maximumSequence).contains(value.sequence),
              now.isFinite, now >= session.startedUptime, now < session.deadlineUptime,
              value.phaseDeadlineUptime.isFinite, value.phaseDeadlineUptime > now,
              value.phaseDeadlineUptime <= session.deadlineUptime,
              value.phaseDeadlineUptime <= now + 5,
              AppUIDriverOperationPolicy.operations(for: value.phase).contains(value.operation),
              value.window == AppUIDriverOperationPolicy.window(for: value.operation) else {
            throw AppUIDriverFailure.invalidRequest
        }
    }

    static func ready(_ value: AppUIDriverReady, session: AppUIDriverSession,
                      sessionSHA256: String) throws {
        guard value.schemaVersion == 1, value.kind == "native-app-ui-driver-ready",
              value.sessionSHA256 == sessionSHA256, hexDigest(sessionSHA256),
              value.nonce == session.nonce, value.driver == session.driver,
              value.failureCode == (value.trusted ? nil : .accessibilityUntrusted),
              (value.codeRequirement != nil) != (value.codeRequirementUnavailableReason != nil),
              value.codeRequirement.map({ !$0.isEmpty && $0.utf8.count <= 4096 && !$0.contains("\0") }) ?? true,
              value.codeRequirementUnavailableReason.map({ !$0.isEmpty && $0.utf8.count <= 128 && !$0.contains("\0") }) ?? true
        else { throw AppUIDriverFailure.invalidReply }
    }
}
#endif
