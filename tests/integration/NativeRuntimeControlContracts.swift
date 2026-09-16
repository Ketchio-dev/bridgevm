import Foundation

enum NativeRuntimeControlContracts {
    static func handler(control: URL, mode: String) -> NativeRuntimeRequestRouter.ControlHandler? {
        guard mode == "control" || mode == "control-slow" else { return nil }
        let state = State(control: control)
        return { request, context in
            try context.validateAdmission()
            let response = try state.respond(request)
            if mode == "control-slow", request.operation == .stop {
                // Deliberately acknowledge task cancellation late to exercise slot retention.
                let delay = Task.detached { try? await Task.sleep(nanoseconds: 5_000_000_000) }
                await delay.value
            }
            return response
        }
    }

    private final class State: @unchecked Sendable {
        private let lock = NSLock()
        private let control: URL
        private var requests: [String: NativeRuntimeControlRequest] = [:]
        init(control: URL) { self.control = control }

        func respond(_ request: NativeRuntimeControlRequest) throws -> NativeRuntimeControlResponse {
            lock.lock(); defer { lock.unlock() }
            let known = requests[request.operationID]
            var refusal: NativeRuntimeStopRefusal?
            if let known, known.target != request.target || known.vmID != request.vmID { refusal = .operationConflict }
            if known == nil, request.operation == .stopStatus { refusal = .operationUnknown }
            if known == nil, request.operation == .stop {
                requests[request.operationID] = request
                try Data().write(to: control.appendingPathComponent("admitted-" + request.operationID), options: .withoutOverwriting)
            }
            let observation = NativeRuntimeStopObservation(operationID: request.operationID, phase: .guestGrace,
                acceptedUptime: 10, deadlineUptime: 202, supervisorComplete: nil, runnerExit: nil, failure: nil)
            return .init(schema: NativeRuntimeControlCodec.responseSchema, scope: NativeRuntimeControlCodec.scope,
                requestID: request.requestID, library: request.library, vmID: request.vmID,
                appInstanceID: request.target.appInstanceID, requestedOperationID: request.operationID,
                target: request.target, disposition: refusal != nil ? .refused
                    : request.operation == .stopStatus ? .status : known == nil ? .accepted : .existing,
                observation: refusal == nil ? observation : nil, refusal: refusal)
        }
    }
}
