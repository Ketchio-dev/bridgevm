import Foundation

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
                try NativeRuntimeControlCodec.validate(response, for: request)
                guard now() < deadline else { break }
                last = response
                if let refusal = response.refusal {
                    if refusal == .operationUnknown, operation == .stopStatus, !admissionKnown {
                        operation = .stop
                    } else { return Result(operationID: operationID, response: response, failure: refusal.rawValue) }
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
