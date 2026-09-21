import CryptoKit
import Foundation

struct T17FirstReadyObservation {
    let readyLine: String?
    let runtimeState: String?
    let startFailure: String?
    let applicationRunning: Bool

    static func capture(readyLine: String?, applicationRunning: Bool,
                        ui: T17UIControlling) throws -> Self {
        guard readyLine == nil, applicationRunning else {
            return Self(readyLine: readyLine, runtimeState: nil, startFailure: nil,
                        applicationRunning: applicationRunning)
        }
        let values = try ui.optionalTexts(["bridgevm.windows.runtime.state", "bridgevm.windows.runtime.start.failure"])
        return Self(readyLine: nil, runtimeState: values["bridgevm.windows.runtime.state"],
                    startFailure: values["bridgevm.windows.runtime.start.failure"],
                    applicationRunning: true)
    }
}

enum T17FirstReadyDecision: Equatable {
    case pending
    case ready(String)
    case failed(code: String, reason: String)
}

struct T17FirstReadyMonitor {
    private(set) var observedActiveRuntime = false

    mutating func evaluate(_ observation: T17FirstReadyObservation) -> T17FirstReadyDecision {
        if let ready = observation.readyLine, !ready.isEmpty { return .ready(ready) }
        guard observation.applicationRunning else {
            return .failed(code: "app-launch-failed", reason: "product app exited during first boot")
        }
        if let failure = observation.startFailure, !failure.isEmpty {
            return .failed(code: "guest-evidence-missing",
                           reason: "runtime start failed; \(Self.digest(failure))")
        }
        switch observation.runtimeState {
        case "start-pending", "booting", "connected":
            observedActiveRuntime = true
        case "timed-out":
            return .failed(code: "guest-evidence-missing", reason: "runtime_state=timed-out")
        case "stopping" where observedActiveRuntime:
            return .failed(code: "guest-evidence-missing", reason: "runtime_state=stopping-after-active")
        case "stopped" where observedActiveRuntime:
            return .failed(code: "guest-evidence-missing", reason: "runtime_state=stopped-after-active")
        default:
            break
        }
        return .pending
    }

    private static func digest(_ value: String) -> String {
        let all = Data(value.utf8); let captured = all.prefix(4_096)
        let hash = SHA256.hash(data: captured).map { String(format: "%02x", $0) }.joined()
        return "ui_failure_bytes=\(all.count),captured=\(captured.count),truncated=\(captured.count < all.count ? 1 : 0),sha256=\(hash)"
    }
}

enum T17FirstReadyWaiter {
    static func wait(timeout: TimeInterval = 600,
                     observe: () throws -> T17FirstReadyObservation,
                     diagnostic: () -> String,
                     now: () -> Date = Date.init,
                     pause: () -> Void = { RunLoop.current.run(until: Date().addingTimeInterval(0.5)) }) throws -> String {
        let deadline = now().addingTimeInterval(timeout)
        var monitor = T17FirstReadyMonitor()
        repeat {
            switch monitor.evaluate(try observe()) {
            case let .ready(line): return line
            case let .failed(code, reason):
                throw T17Blocker(code: code, detail: "\(reason); \(diagnostic())")
            case .pending: pause()
            }
        } while now() < deadline
        throw T17Blocker(code: "guest-evidence-missing",
                         detail: "first boot has no BVAGENT READY/PONG evidence; \(diagnostic())")
    }
}
