import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeRuntimeStartCodecTests: XCTestCase {
    private typealias F = NativeRuntimeStartTestSupport

    func testStartIsCanonicalAndSeparateFromStatusAndStop() throws {
        let request = F.request(), data = try NativeRuntimeCodec.encode(request)
        try NativeRuntimeStartCodec.validate(request)
        XCTAssertEqual(try NativeRuntimeCodec.decode(NativeRuntimeStartRequest.self, from: data, limit: 8192), request)
        XCTAssertThrowsError(try NativeRuntimeCodec.decode(NativeRuntimeRequest.self, from: data, limit: 8192))
        XCTAssertThrowsError(try NativeRuntimeCodec.decode(NativeRuntimeControlRequest.self, from: data, limit: 8192))
        let text = String(decoding: data, as: UTF8.self)
        for raw in [" " + text, String(text.dropLast()) + ",\"target\":{}}",
                    text.replacingOccurrences(of: "\"operation\":\"start\"", with: "\"operation\":\"start\",\"operation\":\"start\""),
                    text.replacingOccurrences(of: "\"operation\":\"start\"", with: "\"operation\":\"stop\"")] {
            XCTAssertThrowsError(try NativeRuntimeCodec.decode(NativeRuntimeStartRequest.self, from: Data(raw.utf8), limit: 8192))
        }
        XCTAssertFalse(text.contains("processID")); XCTAssertFalse(text.contains("runToken"))
    }

    func testMandatoryBindingAndResponseCorrelation() throws {
        for request in [F.request(id: "bad"), F.request(app: F.app.lowercased()), F.request(digest: "bad"), F.request(vmID: "../vm")] {
            XCTAssertThrowsError(try NativeRuntimeStartCodec.validate(request))
        }
        let request = F.request(), response = F.response(request)
        try NativeRuntimeStartCodec.validate(response, for: request)
        XCTAssertThrowsError(try NativeRuntimeStartCodec.validate(response, for: F.request(id: request.operationID)))
        let otherObservation = F.observation(F.request())
        XCTAssertThrowsError(try NativeRuntimeStartCodec.validate(F.response(request, observation: otherObservation,
            disposition: .existing), for: request))
        let status = F.request(.startStatus, id: request.operationID)
        XCTAssertThrowsError(try NativeRuntimeStartCodec.validate(F.response(status, disposition: .accepted), for: status))
        try NativeRuntimeStartCodec.validate(F.response(status), for: status)
    }

    func testStartedRequiresExactInitialHelperProofAndNoPendingWork() throws {
        let request = F.request()
        let valid = F.observation(request, phase: .started, target: F.target, proof: F.proof(), worker: false)
        try NativeRuntimeStartCodec.validate(valid) // Historical proof survives later cleanup.
        for proof in [F.proof(target: .init(token: UUID().uuidString, processID: 123)),
                      F.proof(digest: "bad"), F.proof(helper: 0), F.proof(helper: F.target.processID), F.proof(generation: 1)] {
            XCTAssertThrowsError(try NativeRuntimeStartCodec.validate(F.observation(request, phase: .started,
                target: F.target, proof: proof, worker: false)))
        }
        for value in [F.observation(request, phase: .started, target: F.target, worker: false),
                      F.observation(request, phase: .started, proof: F.proof(), worker: false),
                      F.observation(request, phase: .started, target: F.target, proof: F.proof()),
                      F.observation(request, phase: .started, target: F.target, proof: F.proof(), failure: .deadlineExceeded, worker: false)] {
            XCTAssertThrowsError(try NativeRuntimeStartCodec.validate(value))
        }
    }

    func testDeadlineAndUncertaintyCannotBecomeSuccessfulOrSafe() throws {
        let request = F.request()
        for deadline in [10, 39, 41, Double.infinity, Double.nan] {
            XCTAssertThrowsError(try NativeRuntimeStartCodec.validate(F.observation(request, deadline: deadline)))
        }
        try NativeRuntimeStartCodec.validate(F.observation(request, phase: .unconfirmed, failure: .deadlineExceeded))
        try NativeRuntimeStartCodec.validate(F.observation(request, phase: .failed, failure: .deadlineExceeded, worker: false))
        for value in [F.observation(request, target: F.target),
                      F.observation(request, phase: .awaitingStartup, worker: false, owned: true),
                      F.observation(request, phase: .failed, failure: .preparation),
                      F.observation(request, phase: .failed, failure: .preparation, worker: false, owned: true),
                      F.observation(request, phase: .unconfirmed, failure: .preparation, worker: false)] {
            XCTAssertThrowsError(try NativeRuntimeStartCodec.validate(value))
        }
    }
}
