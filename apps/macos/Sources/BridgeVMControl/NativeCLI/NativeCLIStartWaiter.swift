import Foundation

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
                appInstanceID: initial.appInstanceID,
                expectedSavedConfigurationDigest: initial.expectedSavedConfigurationDigest,
                operationID: initial.operationID)
            do {
                let response = try query(request)
                try NativeRuntimeStartCodec.validate(response, for: request)
                guard now() < deadline else { break }
                last = response
                if let refusal = response.refusal {
                    if refusal == .operationUnknown, operation == .startStatus, !admissionKnown {
                        operation = .start
                    } else { return Result(response: response, failure: refusal.rawValue) }
                } else if let observation = response.observation {
                    admissionKnown = true
                    if observation.phase == .started { return Result(response: response, failure: nil) }
                    if observation.phase == .failed || observation.phase == .unconfirmed {
                        return Result(response: response, failure: observation.failure?.rawValue ?? "startupUnconfirmed")
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
