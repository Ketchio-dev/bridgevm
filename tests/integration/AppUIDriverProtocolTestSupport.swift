import Darwin
import Foundation

enum AppUIDriverProtocolTestSupport {
    static var checks = 0
    static var lastCheck = "initial"
    static func check(_ condition: @autoclosure () throws -> Bool, _ message: String) throws {
        lastCheck = message
        guard try condition() else { throw NSError(domain: message, code: 1) }
        checks += 1
    }
    static func refuses(_ message: String, _ action: () throws -> Void) throws {
        lastCheck = message
        do { try action() }
        catch { checks += 1; return }
        throw NSError(domain: "Expected refusal: " + message, code: 1)
    }
    static func session(root: String = "/fixture/app-ui-private") -> AppUIDriverSession {
        func identity(_ driver: Bool) -> AppUIDriverProcessIdentity {
            let bundle = root + (driver ? "/BridgeVMAppUIDriver.app" : "/BridgeVMAppUIHost.app")
            return AppUIDriverProcessIdentity(pid: driver ? 22 : 11, launchDate: 1_700_000_000,
                bundleIdentifier: driver ? AppUIDriverConstants.driverBundleIdentifier : AppUIDriverConstants.hostBundleIdentifier,
                bundlePath: bundle, executablePath: bundle + "/Contents/MacOS/" + (driver ? "AppUIHostLauncher" : "BridgeVMControl"),
                executableSHA256: String(repeating: driver ? "b" : "a", count: 64))
        }
        return AppUIDriverSession(nonce: String(repeating: "c", count: 64), startedUptime: 100,
            deadlineUptime: 190, host: identity(false), driver: identity(true))
    }
    static func hash(_ value: AppUIDriverSession) throws -> String {
        AppUIDriverCanonicalJSON.digest(try AppUIDriverCanonicalJSON.encode(value))
    }
    static func request(_ operation: AppUIDriverOperation = .welcomeControls,
                        phase: AppUIDriverPhase = .welcome, sequence: UInt32 = 1,
                        deadline: Double = 105, session: AppUIDriverSession = session()) throws -> AppUIDriverRequest {
        AppUIDriverRequest(sessionSHA256: try hash(session), nonce: session.nonce, sequence: sequence,
            phase: phase, phaseDeadlineUptime: deadline,
            window: AppUIDriverOperationPolicy.window(for: operation), operation: operation)
    }
    static func reply(_ request: AppUIDriverRequest, values: AppUIDriverValues? = nil,
                      failure: AppUIDriverFailure? = nil, session: AppUIDriverSession = session()) throws -> AppUIDriverReply {
        AppUIDriverReply(sessionSHA256: request.sessionSHA256, nonce: request.nonce, sequence: request.sequence,
            requestSHA256: AppUIDriverCanonicalJSON.digest(try AppUIDriverCanonicalJSON.encode(request)),
            hostPID: session.host.pid, driverPID: session.driver.pid, operation: request.operation,
            outcome: failure != nil ? .refused : AppUIDriverOperationPolicy.isMutation(request.operation) ? .performed : .observed,
            values: values, failureCode: failure, axError: failure == .axFailure ? -25204 : nil)
    }
    static func ready(_ session: AppUIDriverSession, trusted: Bool) throws -> AppUIDriverReady {
        AppUIDriverReady(sessionSHA256: try hash(session), nonce: session.nonce, driver: session.driver,
            trusted: trusted, failureCode: trusted ? nil : .accessibilityUntrusted,
            codeRequirement: nil, codeRequirementUnavailableReason: "not-observed-in-pure-test")
    }
    static func temporaryRoot() throws -> URL {
        guard let resolved = realpath(FileManager.default.temporaryDirectory.path, nil) else {
            throw AppUIDriverFailure.ioFailure
        }
        defer { free(resolved) }
        let parent = URL(fileURLWithPath: String(cString: resolved))
            .appendingPathComponent("bridgevm-ui-protocol-" + UUID().uuidString)
        let root = parent.appendingPathComponent("app-ui-private")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700])
        return root
    }
    static func rawFile(_ path: URL, data: Data) throws {
        try data.write(to: path, options: .withoutOverwriting)
        guard chmod(path.path, 0o600) == 0 else { throw AppUIDriverFailure.ioFailure }
    }
}
