import Foundation

struct NativeCLIRuntimeStop: Encodable {
    let schema = "bridgevm.app-stop.v1"
    let scope = NativeRuntimeControlCodec.scope
    let vmID: String
    let libraryPath: String
    let target: NativeRuntimeStopTarget?
    let operationID: String?
    let complete: Bool
    let observation: NativeRuntimeStopObservation?
    let unavailableReason: String?

    static func run(rootURL: URL, id: String) -> Self {
        let deadline = NativeRuntimeTransport.now + 210
        var target: NativeRuntimeStopTarget?, operationID: String?
        do {
            let library = try NativeRuntimeLibraryHandle.open(rootURL: rootURL, create: false)
            let saved = NativeCLIStatusConfiguration.read(library: library, id: id)
            let request = NativeRuntimeRequest(schema: NativeRuntimeCodec.requestSchema, operation: "status",
                requestID: UUID().uuidString, library: library.identity, vmID: id, savedConfiguration: saved)
            let status = try NativeRuntimeClient.query(rootURL: rootURL, request: request)
            let accepted = try selectTarget(status)
            let requestedOperation = UUID().uuidString
            target = accepted; operationID = requestedOperation
            let stop = NativeRuntimeControlRequest(schema: NativeRuntimeControlCodec.requestSchema,
                operation: .stop, requestID: UUID().uuidString, library: library.identity, vmID: id,
                operationID: requestedOperation, target: accepted)
            let result = NativeCLIStopWaiter.wait(request: stop, deadline: deadline) {
                try NativeRuntimeControlClient.query(rootURL: rootURL, request: $0)
            }
            try library.validateCurrentIdentity()
            return Self(vmID: id, libraryPath: library.identity.canonicalPath, target: accepted,
                operationID: result.operationID, complete: result.complete,
                observation: result.response?.observation, unavailableReason: result.failure)
        } catch {
            return Self(vmID: id, libraryPath: rootURL.path, target: target, operationID: operationID,
                complete: false, observation: nil,
                unavailableReason: (error as? NativeRuntimeStopRefused)?.reason
                    ?? (error as? NativeRuntimeError)?.rawValue ?? "transportFailure")
        }
    }

    static func selectTarget(_ status: NativeRuntimeResponse) throws -> NativeRuntimeStopTarget {
        let active = status.sessions.filter { $0.ownership == .owned }
        if active.isEmpty, status.sessions.contains(where: { $0.ownership == .attachedObservation }) {
            throw NativeRuntimeStopRefused.unsupportedRuntime
        }
        let candidates = active.isEmpty ? status.sessions.filter { $0.ownership == .ownedExitObserved } : active
        guard !candidates.isEmpty else { throw NativeRuntimeStopRefused.targetUnavailable }
        guard candidates.count == 1, let session = candidates.first else { throw NativeRuntimeStopRefused.ambiguousTarget }
        guard let process = session.ownedProcess ?? session.lastOwnedExit?.process,
              let digest = session.acceptedConfigurationDigest else { throw NativeRuntimeStopRefused.configurationChanged }
        return .init(appInstanceID: status.appInstanceID, runToken: process.token,
                     processID: process.processID, acceptedConfigurationDigest: digest)
    }
}

extension NativeCLIRuntimeStop {
    var text: String {
        var lines = ["Native VM stop: \(vmID)", "Observation scope: \(scope)",
                     "Owned runtime cleanup: \(complete ? "confirmed" : "not confirmed")"]
        if let operationID { lines.append("Operation: \(operationID)") }
        if let target { lines.append("Owned run: \(target.runToken), PID \(target.processID)") }
        if let observation { lines.append("Stop phase: \(observation.phase.rawValue)") }
        if let unavailableReason { lines.append("Stop result unavailable: \(unavailableReason)") }
        lines.append("Completion requires supervisor cleanup and retained runner exit; normal guest shutdown is unproven.")
        return lines.joined(separator: "\n") + "\n"
    }
}

enum NativeCLIStopWaiter {
    struct Result {
        let operationID: String
        let response: NativeRuntimeControlResponse?
        let failure: String?
        var complete: Bool { failure == nil && response?.observation?.phase == .completed }
    }

    static func wait(request initial: NativeRuntimeControlRequest,
                     deadline suppliedDeadline: TimeInterval? = nil,
                     now: () -> TimeInterval = { NativeRuntimeTransport.now },
                     pause: (TimeInterval) -> Void = { Thread.sleep(forTimeInterval: $0) },
                     query: (NativeRuntimeControlRequest) throws -> NativeRuntimeControlResponse) -> Result {
        let deadline = suppliedDeadline ?? now() + 210
        var operationID = initial.operationID, operation = NativeRuntimeControlRequest.Operation.stop
        var last: NativeRuntimeControlResponse?, admissionKnown = false
        while now() < deadline {
            let request = NativeRuntimeControlRequest(schema: initial.schema, operation: operation,
                requestID: UUID().uuidString, library: initial.library, vmID: initial.vmID,
                operationID: operationID, target: initial.target)
            do {
                let response = try query(request)
                guard now() < deadline else { break }
                last = response
                if let refusal = response.refusal {
                    if refusal == .operationUnknown, operation == .stopStatus, !admissionKnown {
                        operation = .stop
                    } else {
                        return Result(operationID: operationID, response: response, failure: response.reason ?? refusal.rawValue)
                    }
                } else if let observation = response.observation {
                    admissionKnown = true
                    operationID = observation.operationID
                    if observation.phase == .completed {
                        return Result(operationID: operationID, response: response, failure: nil)
                    }
                    if observation.phase == .unconfirmed {
                        return Result(operationID: operationID, response: response,
                                      failure: observation.failure?.rawValue ?? "cleanupUnconfirmed")
                    }
                    operation = .stopStatus
                }
            } catch {
                let failure = (error as? NativeRuntimeError) ?? .transportFailure
                guard failure == .timedOut || failure == .transportFailure else {
                    return Result(operationID: operationID, response: last, failure: failure.rawValue)
                }
                operation = .stopStatus
            }
            let remaining = deadline - now()
            if remaining > 0 { pause(min(0.1, remaining)) }
        }
        return Result(operationID: operationID, response: last, failure: "deadlineExceeded")
    }
}
