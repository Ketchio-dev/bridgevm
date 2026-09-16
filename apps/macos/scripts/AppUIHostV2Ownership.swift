import Foundation

enum AppUIHostV2Role: String, CaseIterable {
    case host, driver
    var bundleName: String { self == .host ? "BridgeVMAppUIHost.app" : "BridgeVMAppUIDriver.app" }
    var executableName: String { self == .host ? "BridgeVMControl" : "AppUIHostLauncher" }
    var identifier: String { "dev.bridgevm.app-ui-" + rawValue }
}

// Pure policy. The caller supplies fresh complete identity before every signal.
struct AppUIHostV2Ownership {
    enum Action: Equatable { case wait, terminate, kill, finish }
    let role: AppUIHostV2Role
    let bundlePath: String
    let executableSHA256: String
    let requestedAt: Double
    private(set) var launchRequested = false
    private(set) var identity: AppUIDriverProcessIdentity?
    private(set) var exitObserved = false
    private(set) var noLaunchVerified = false
    private(set) var finished = false
    private(set) var failure: String?
    private(set) var termination = "none"
    private var conflict = false
    private var stopRequestedAt: Double?
    private var signalledAt: Double?

    init(role: AppUIHostV2Role, bundlePath: String, executableSHA256: String, requestedAt: Double) {
        self.role = role; self.bundlePath = bundlePath
        self.executableSHA256 = executableSHA256; self.requestedAt = requestedAt
    }

    var executablePath: String { bundlePath + "/Contents/MacOS/" + role.executableName }
    var cleanupVerified: Bool {
        !conflict && ((identity != nil && exitObserved) || (!launchRequested && noLaunchVerified))
    }
    var success: Bool { cleanupVerified && identity != nil && failure == nil && finished }

    mutating func willLaunch() {
        precondition(!launchRequested && !finished && stopRequestedAt == nil)
        launchRequested = true
    }

    func matches(_ value: AppUIDriverProcessIdentity) -> Bool {
        value.pid > 1 && value.launchDate.isFinite && value.launchDate >= requestedAt
            && value.bundleIdentifier == role.identifier && value.bundlePath == bundlePath
            && value.executablePath == executablePath && value.executableSHA256 == executableSHA256
    }

    mutating func observe(_ value: AppUIDriverProcessIdentity, now: Double) -> Bool {
        if finished, identity == value, matches(value) { return false }
        guard !finished, launchRequested, matches(value) else {
            refuseObservation("Launched application identity differs", now: now)
            return false
        }
        if let identity, identity != value {
            refuseObservation("More than one application identity appeared", now: now)
            return false
        }
        identity = value
        return true
    }

    mutating func refuseObservation(_ reason: String, now: Double) {
        conflict = true
        failure = failure ?? reason
        requestStop(reason, now: now)
    }

    mutating func requestStop(_ reason: String, now: Double) {
        guard !finished else { return }
        failure = failure ?? reason
        stopRequestedAt = stopRequestedAt ?? now
        if !launchRequested {
            noLaunchVerified = true
            finished = true
        }
    }

    mutating func poll(now: Double, exited: Bool) -> Action {
        guard !finished else { return .finish }
        if identity != nil && exited {
            exitObserved = true
            finished = true
            return .finish
        }
        guard let stopped = stopRequestedAt else { return .wait }
        guard identity != nil else {
            if now >= stopped + 5 {
                failure = (failure ?? "Launch failed") + "; ownership remains unresolved"
                finished = true
                return .finish
            }
            return .wait
        }
        if termination == "none" { return .terminate }
        if termination == "term", let signalledAt, now >= signalledAt + 3 { return .kill }
        if termination == "kill", let signalledAt, now >= signalledAt + 2 {
            failure = (failure ?? "Launch failed") + "; process exit was not observed"
            finished = true
            return .finish
        }
        return .wait
    }

    mutating func authorize(_ action: Action, current: AppUIDriverProcessIdentity?, now: Double) -> Int32? {
        guard !finished, stopRequestedAt != nil else { return nil }
        guard let identity, current == identity, matches(identity),
              (action == .terminate && termination == "none")
                || (action == .kill && termination == "term" && now >= (signalledAt ?? .infinity) + 3) else {
            conflict = true
            failure = (failure ?? "Termination refused") + "; fresh complete identity unavailable"
            finished = true
            return nil
        }
        termination = action == .terminate ? "term" : "kill"
        signalledAt = now
        return identity.pid
    }

    var receipt: [String: Any] {
        ["pid": identity.map { Int($0.pid) as Any } ?? NSNull(),
         "launch_date": identity.map { $0.launchDate as Any } ?? NSNull(),
         "bundle_identifier": role.identifier, "bundle_path": bundlePath,
         "executable_path": executablePath, "executable_sha256": executableSHA256,
         "identity_verified": identity != nil, "exit_observed": exitObserved,
         "cleanup_verified": cleanupVerified, "termination": termination,
         "launch_requested": launchRequested, "no_launch_verified": noLaunchVerified,
         "failure": failure.map { $0 as Any } ?? NSNull()]
    }
}
