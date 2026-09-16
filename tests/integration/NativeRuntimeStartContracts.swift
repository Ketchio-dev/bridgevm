import Foundation

enum NativeRuntimeStartContracts {
    static func serve(_ owner: NativeRuntimeOwner, control: URL, mode: String,
                      status: @escaping NativeRuntimeServer.Handler) throws {
        try owner.start(controlHandler: NativeRuntimeControlContracts.handler(control: control,
            mode: mode.hasPrefix("start") ? "control" : mode), startHandler: handler(control: control, mode: mode), handler: status)
    }

    private static func handler(control: URL, mode: String) -> NativeRuntimeRequestRouter.StartHandler? {
        guard mode == "start" || mode == "start-slow" else { return nil }
        let state = State(control: control)
        return { request, context in
            try context.validateAdmission()
            let response = try state.respond(request)
            if mode == "start-slow", request.operation == .start {
                let delay = Task.detached { try? await Task.sleep(nanoseconds: 5_000_000_000) }
                await delay.value
            }
            return response
        }
    }

    private final class State: @unchecked Sendable {
        private let mutex = NSLock()
        private let control: URL
        private let app = "A0000000-0000-0000-0000-000000000001"
        private var requests: [String: NativeRuntimeStartRequest] = [:]
        init(control: URL) { self.control = control }

        func respond(_ request: NativeRuntimeStartRequest) throws -> NativeRuntimeStartResponse {
            mutex.lock(); defer { mutex.unlock() }
            let known = requests[request.operationID]
            var refusal: NativeRuntimeStartRefusal?
            if request.appInstanceID != app { refusal = .ownerChanged }
            else if let known, known.vmID != request.vmID || known.expectedSavedConfigurationDigest != request.expectedSavedConfigurationDigest {
                refusal = .operationConflict
            } else if known == nil, request.operation == .startStatus { refusal = .operationUnknown }
            if refusal == nil, known == nil, request.operation == .start {
                requests[request.operationID] = request
                try Data().write(to: control.appendingPathComponent("start-admitted-" + request.operationID), options: .withoutOverwriting)
            }
            let observation = NativeRuntimeStartObservation(operationID: request.operationID,
                expectedSavedConfigurationDigest: request.expectedSavedConfigurationDigest,
                phase: .preparing, acceptedUptime: 10, deadlineUptime: 40, target: nil, proof: nil,
                failure: nil, workerPending: true, mayHaveOwnedWork: false)
            return .init(schema: NativeRuntimeStartCodec.responseSchema, scope: NativeRuntimeStartCodec.scope,
                requestID: request.requestID, library: request.library, vmID: request.vmID, appInstanceID: app,
                expectedSavedConfigurationDigest: request.expectedSavedConfigurationDigest, requestedOperationID: request.operationID,
                disposition: refusal != nil ? .refused : request.operation == .startStatus ? .status : known == nil ? .accepted : .existing,
                observation: refusal == nil ? observation : nil, refusal: refusal)
        }
    }
}
