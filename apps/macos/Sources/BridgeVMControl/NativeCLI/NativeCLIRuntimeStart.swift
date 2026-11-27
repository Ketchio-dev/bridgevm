import Foundation

struct NativeCLIRuntimeStart: Encodable {
    let schema = "bridgevm.app-start.v2"
    let scope = NativeRuntimeStartCodec.scope
    let vmID: String
    let libraryPath: String
    let appInstanceID: String?
    let operationID: String
    let started: Bool
    let observation: NativeRuntimeStartObservation?
    let unavailableReason: String?

    /// Sends only the VM ID; the app starts that VM's current saved configuration.
    static func run(rootURL: URL, id: String) -> Self {
        let deadline = NativeRuntimeTransport.now + 30
        let operationID = UUID().uuidString
        var appInstanceID: String?
        do {
            let library = try NativeRuntimeLibraryHandle.open(rootURL: rootURL, create: false)
            let statusRequest = NativeRuntimeRequest(schema: NativeRuntimeCodec.requestSchema, operation: "status",
                requestID: UUID().uuidString, library: library.identity, vmID: id,
                savedConfiguration: NativeCLIStatusConfiguration.read(library: library, id: id))
            let status = try NativeRuntimeClient.query(rootURL: rootURL, request: statusRequest)
            appInstanceID = status.appInstanceID
            let request = NativeRuntimeStartRequest(schema: NativeRuntimeStartCodec.requestSchema,
                operation: .start, requestID: UUID().uuidString, library: library.identity, vmID: id,
                appInstanceID: status.appInstanceID, operationID: operationID)
            let result = NativeCLIStartWaiter.wait(request: request, deadline: deadline) {
                try NativeRuntimeStartClient.query(rootURL: rootURL, request: $0)
            }
            return Self(vmID: id, libraryPath: library.identity.canonicalPath, appInstanceID: appInstanceID,
                operationID: operationID, started: result.started,
                observation: result.response?.observation, unavailableReason: result.failure)
        } catch {
            return Self(vmID: id, libraryPath: rootURL.path, appInstanceID: appInstanceID,
                operationID: operationID, started: false, observation: nil,
                unavailableReason: (error as? NativeRuntimeError)?.rawValue ?? "transportFailure")
        }
    }
}

extension NativeCLIRuntimeStart {
    var text: String {
        var lines = ["Native VM start: \(vmID)", "Observation scope: \(scope)", "Runtime startup: \(started ? "confirmed" : "not confirmed")", "Operation: \(operationID)"]
        if let observation { lines.append("Start phase: \(observation.phase.rawValue)") }
        if let target = observation?.target { lines.append("Owned run: \(target.token), PID \(target.processID)") }
        if let unavailableReason {
            lines.append("Start result unavailable: "
                + NativeCLIReason.sentence(unavailableReason, vmID: vmID, action: "starting"))
        }
        lines.append("Startup confirms the supervisor and initial helper; guest boot and display readiness remain unproven.")
        return lines.joined(separator: "\n") + "\n"
    }
}

enum NativeCLIStartWaiter {
    struct Result {
        let response: NativeRuntimeStartResponse?
        let failure: String?
        var started: Bool { failure == nil && response?.observation?.phase == .started }
    }

    static func wait(request initial: NativeRuntimeStartRequest,
                     deadline suppliedDeadline: TimeInterval? = nil,
                     now: () -> TimeInterval = { NativeRuntimeTransport.now },
                     pause: (TimeInterval) -> Void = { Thread.sleep(forTimeInterval: $0) },
                     query: (NativeRuntimeStartRequest) throws -> NativeRuntimeStartResponse) -> Result {
        let deadline = suppliedDeadline ?? now() + 30
        var operation = NativeRuntimeStartRequest.Operation.start
        var last: NativeRuntimeStartResponse?, admissionKnown = false
        while now() < deadline {
            let request = NativeRuntimeStartRequest(schema: initial.schema, operation: operation,
                requestID: UUID().uuidString, library: initial.library, vmID: initial.vmID,
                appInstanceID: initial.appInstanceID, operationID: initial.operationID)
            do {
                let response = try query(request)
                guard now() < deadline else { break }
                last = response
                if let refusal = response.refusal {
                    if refusal == .operationUnknown, operation == .startStatus, !admissionKnown {
                        operation = .start
                    } else { return Result(response: response, failure: response.reason ?? refusal.rawValue) }
                } else if let observation = response.observation {
                    admissionKnown = true
                    if observation.phase == .started { return Result(response: response, failure: nil) }
                    if observation.phase == .failed {
                        return Result(response: response, failure: observation.failure ?? "startupFailed")
                    }
                    operation = .startStatus
                }
            } catch {
                let failure = (error as? NativeRuntimeError) ?? .transportFailure
                guard failure == .timedOut || failure == .transportFailure else {
                    return Result(response: last, failure: failure.rawValue)
                }
                operation = .startStatus
            }
            let remaining = deadline - now()
            if remaining > 0 { pause(min(0.1, remaining)) }
        }
        return Result(response: last, failure: "deadlineExceeded")
    }
}
