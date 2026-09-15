import Foundation

struct AppUIHostProcessIdentity: Equatable {
    let pid: Int32
    let launchDate: TimeInterval
    let bundleIdentifier: String
    let bundlePath: String
    let executablePath: String
    let executableSHA256: String
}

struct AppUIHostLaunchExpectation {
    let bundlePath: String
    let executableSHA256: String
    let requestedAt: TimeInterval
    var executablePath: String { bundlePath + "/Contents/MacOS/BridgeVMControl" }

    func matches(_ identity: AppUIHostProcessIdentity) -> Bool {
        identity.pid > 1 && identity.launchDate.isFinite && identity.launchDate >= requestedAt
            && identity.bundleIdentifier == "dev.bridgevm.app-ui-host"
            && identity.bundlePath == bundlePath && identity.executablePath == executablePath
            && identity.executableSHA256 == executableSHA256
    }
}

// Pure lifecycle policy: no AppKit, process discovery, signals, or file access.
// Callers must provide a fresh complete identity immediately before each signal.
struct AppUIHostLaunchOwnership {
    enum Action: Equatable { case wait, terminate, kill, finish }
    enum StopReason { case cancelled, deadline, failure(String) }
    let expectation: AppUIHostLaunchExpectation
    let startedAt: TimeInterval
    private(set) var identity: AppUIHostProcessIdentity?
    private(set) var cancelled = false
    private(set) var timedOut = false
    private(set) var exitObserved = false
    private(set) var finished = false
    private(set) var termination = "none"
    private(set) var failure: String?
    private var stopRequestedAt: TimeInterval?
    private var signalledAt: TimeInterval?
    private var ownershipConflict = false

    init(expectation: AppUIHostLaunchExpectation, startedAt: TimeInterval) {
        self.expectation = expectation
        self.startedAt = startedAt
    }

    var cleanupVerified: Bool { identity != nil && exitObserved && !ownershipConflict }
    var success: Bool { finished && cleanupVerified && failure == nil && !cancelled && !timedOut }

    mutating func observe(_ candidate: AppUIHostProcessIdentity, now: TimeInterval) -> Bool {
        guard !finished else { return false }
        guard expectation.matches(candidate) else {
            requestStop(.failure("Launch identity did not match the sealed diagnostic host"), now: now)
            return false
        }
        if let identity, identity != candidate {
            ownershipConflict = true
            requestStop(.failure("More than one diagnostic host identity appeared"), now: now)
            return false
        }
        identity = candidate
        return true
    }

    mutating func requestStop(_ reason: StopReason, now: TimeInterval) {
        guard !finished else { return }
        switch reason {
        case .cancelled:
            cancelled = true
            failure = failure ?? "Owning job cancelled the native app diagnostic"
        case .deadline:
            timedOut = true
            failure = failure ?? "Native app diagnostic exceeded its 90 second deadline"
        case .failure(let message): failure = failure ?? message
        }
        if stopRequestedAt == nil { stopRequestedAt = now }
    }

    mutating func poll(now: TimeInterval, ownedProcessExited: Bool) -> Action {
        guard !finished else { return .finish }
        if now >= startedAt + 90 { requestStop(.deadline, now: now) }
        if identity != nil && ownedProcessExited {
            exitObserved = true
            finished = true
            return .finish
        }
        guard let stopRequestedAt else { return .wait }
        guard identity != nil else {
            if now >= stopRequestedAt + 5 {
                failure = (failure ?? "Launch failed") + "; process ownership remains unresolved"
                finished = true
                return .finish
            }
            return .wait
        }
        if termination == "none" { return .terminate }
        if termination == "term", let signalledAt, now >= signalledAt + 3 { return .kill }
        if termination == "kill", let signalledAt, now >= signalledAt + 2 {
            failure = (failure ?? "Launch failed") + "; owned process exit was not observed"
            finished = true
            return .finish
        }
        return .wait
    }

    mutating func authorize(_ action: Action, current: AppUIHostProcessIdentity?,
                            now: TimeInterval) -> Int32? {
        guard !finished else { return nil }
        guard let identity, current == identity,
              (action == .terminate && termination == "none")
                || (action == .kill && termination == "term" && now >= (signalledAt ?? .infinity) + 3) else {
            failure = (failure ?? "Termination refused") + "; fresh process identity could not be verified"
            ownershipConflict = true
            finished = true
            return nil
        }
        guard stopRequestedAt != nil else { return nil }
        termination = action == .terminate ? "term" : "kill"
        signalledAt = now
        return identity.pid
    }

    var receipt: [String: Any] {
        ["schema_version": 1, "kind": "native-app-ui-launcher",
         "host_pid": identity.map { Int($0.pid) as Any } ?? NSNull(),
         "host_launch_date": identity.map { $0.launchDate as Any } ?? NSNull(),
         "host_bundle_path": expectation.bundlePath, "host_executable_path": expectation.executablePath,
         "host_executable_sha256": expectation.executableSHA256, "identity_verified": identity != nil,
         "exit_observed": exitObserved, "cleanup_verified": cleanupVerified,
         "cancelled": cancelled, "timed_out": timedOut, "termination": termination,
         "success": success, "failure": failure.map { $0 as Any } ?? NSNull()]
    }
}
