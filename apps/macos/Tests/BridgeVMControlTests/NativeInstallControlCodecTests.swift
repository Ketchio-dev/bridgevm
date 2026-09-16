import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeInstallControlCodecTests: XCTestCase {
    private let library = NativeRuntimeLibraryIdentity(canonicalPath: "/tmp/library", device: 1, inode: 2, uid: 501)
    private let appID = "11111111-1111-1111-1111-111111111111"
    private let installID = "33333333-3333-3333-3333-333333333333"
    private let digest = String(repeating: "a", count: 64)

    private func request(_ operation: NativeInstallControlRequest.Operation = .install,
                         operationID: String? = "33333333-3333-3333-3333-333333333333") -> NativeInstallControlRequest {
        .init(schema: NativeInstallControlCodec.requestSchema, operation: operation,
              requestID: "44444444-4444-4444-4444-444444444444", library: library,
              vmID: "windows", appInstanceID: appID,
              expectedSavedConfigurationDigest: digest,
              operationID: operation == .install ? operationID : nil)
    }

    private func observation(_ phase: NativeInstallObservation.Phase = .preparingPlan,
                             worker: Bool = true, running: Bool = false, canCancel: Bool = true,
                             failure: String? = nil, logs: [String] = []) -> NativeInstallObservation {
        .init(operationID: installID, expectedSavedConfigurationDigest: digest, phase: phase,
              acceptedUptime: 10, workerPending: worker, sessionRunning: running,
              canCancel: canCancel, logTail: logs, failure: failure)
    }

    private func response(_ request: NativeInstallControlRequest,
                          disposition: NativeInstallControlResponse.Disposition = .accepted,
                          observation: NativeInstallObservation? = nil,
                          refusal: NativeInstallControlRefusal? = nil) -> NativeInstallControlResponse {
        .init(schema: NativeInstallControlCodec.responseSchema, scope: NativeInstallControlCodec.scope,
              requestID: request.requestID, library: request.library, vmID: request.vmID,
              appInstanceID: appID, expectedSavedConfigurationDigest: request.expectedSavedConfigurationDigest,
              requestedOperationID: request.operationID, disposition: disposition,
              observation: observation, refusal: refusal)
    }

    func testCanonicalRequestIsSeparateFromRuntimeProtocols() throws {
        let value = request(), data = try NativeRuntimeCodec.encode(value)
        try NativeInstallControlCodec.validate(value)
        XCTAssertEqual(try NativeRuntimeCodec.decode(NativeInstallControlRequest.self, from: data, limit: 8192), value)
        XCTAssertThrowsError(try NativeRuntimeCodec.decode(NativeRuntimeRequest.self, from: data, limit: 8192))
        XCTAssertThrowsError(try NativeRuntimeCodec.decode(NativeRuntimeControlRequest.self, from: data, limit: 8192))
        XCTAssertThrowsError(try NativeRuntimeCodec.decode(NativeRuntimeStartRequest.self, from: data, limit: 8192))
    }

    func testOnlyInstallCarriesAnIdempotencyOperationID() throws {
        XCTAssertThrowsError(try NativeInstallControlCodec.validate(request(operationID: nil)))
        XCTAssertThrowsError(try NativeInstallControlCodec.validate(request(operationID: "bad")))
        for operation in [NativeInstallControlRequest.Operation.installStatus, .installCancel] {
            var value = request(operation)
            try NativeInstallControlCodec.validate(value)
            value = .init(schema: value.schema, operation: operation, requestID: value.requestID,
                          library: value.library, vmID: value.vmID, appInstanceID: value.appInstanceID,
                          expectedSavedConfigurationDigest: value.expectedSavedConfigurationDigest,
                          operationID: installID)
            XCTAssertThrowsError(try NativeInstallControlCodec.validate(value))
        }
    }

    func testResponseCorrelationAndDispositionAreStrict() throws {
        let install = request(), accepted = response(install, observation: observation())
        try NativeInstallControlCodec.validate(accepted, for: install)
        let status = request(.installStatus)
        try NativeInstallControlCodec.validate(response(status, disposition: .status,
            observation: observation(.installing, worker: false, running: true)), for: status)
        XCTAssertThrowsError(try NativeInstallControlCodec.validate(response(status,
            observation: observation()), for: status))
        try NativeInstallControlCodec.validate(response(install, disposition: .refused,
            refusal: .busy), for: install)
        XCTAssertThrowsError(try NativeInstallControlCodec.validate(response(install,
            disposition: .refused, observation: observation(), refusal: .busy), for: install))
    }

    func testObservationRejectsFalseTerminalAndUnboundedOutput() throws {
        try NativeInstallControlCodec.validate(observation())
        try NativeInstallControlCodec.validate(observation(.done, worker: false, canCancel: false))
        try NativeInstallControlCodec.validate(observation(.failed, worker: false, canCancel: false,
            failure: "source changed", logs: ["retained failure"]))
        for value in [observation(.done, worker: false, running: true, canCancel: false),
                      observation(.failed, worker: false, canCancel: false),
                      observation(.installing, worker: false, running: true, canCancel: false, failure: "bad"),
                      observation(logs: Array(repeating: "line", count: 65)),
                      observation(failure: String(repeating: "x", count: 4_097))] {
            XCTAssertThrowsError(try NativeInstallControlCodec.validate(value))
        }
    }
}
