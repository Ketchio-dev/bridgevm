import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeRuntimeControlCodecTests: XCTestCase {
    private typealias Fixture = NativeRuntimeControlTestSupport

    func testCanonicalControlRejectsUnknownDuplicateAndMixedStatusKeys() throws {
        let request = Fixture.request(), bytes = try NativeRuntimeCodec.encode(Fixture.request())
        try NativeRuntimeControlCodec.validate(request)
        XCTAssertNoThrow(try NativeRuntimeCodec.decode(NativeRuntimeControlRequest.self, from: bytes, limit: 8192))
        let text = String(decoding: bytes, as: UTF8.self)
        for value in [" " + text, String(text.dropLast()) + ",\"unknown\":true}",
                      text.replacingOccurrences(of: "\"operation\":\"stop\"", with: "\"operation\":\"stop\",\"operation\":\"stop\""),
                      text.replacingOccurrences(of: "\"operation\":\"stop\"", with: "\"operation\":\"status\"")] {
            XCTAssertThrowsError(try NativeRuntimeCodec.decode(NativeRuntimeControlRequest.self, from: Data(value.utf8), limit: 8192))
        }
        XCTAssertThrowsError(try NativeRuntimeCodec.decode(NativeRuntimeRequest.self, from: bytes, limit: 8192))
    }

    func testTargetAndOperationIdentityAreMandatory() {
        for target in [
            NativeRuntimeStopTarget(appInstanceID: "bad", runToken: Fixture.token, processID: 123, acceptedConfigurationDigest: String(repeating: "a", count: 64)),
            .init(appInstanceID: Fixture.app, runToken: Fixture.token.lowercased(), processID: 123, acceptedConfigurationDigest: String(repeating: "a", count: 64)),
            .init(appInstanceID: Fixture.app, runToken: Fixture.token, processID: 0, acceptedConfigurationDigest: String(repeating: "a", count: 64)),
            .init(appInstanceID: Fixture.app, runToken: Fixture.token, processID: 123, acceptedConfigurationDigest: "bad")
        ] { XCTAssertThrowsError(try NativeRuntimeControlCodec.validate(Fixture.request(target: target))) }
        XCTAssertThrowsError(try NativeRuntimeControlCodec.validate(Fixture.request(id: "bad")))
    }

    func testCompletedRequiresBothProofsAndMatchingRetainedExit() throws {
        let request = Fixture.request()
        for pair in [(false, false), (true, false), (false, true)] {
            let response = Fixture.response(request, observation: Fixture.observation(request, phase: .completed, proof: pair.0, exit: pair.1))
            XCTAssertThrowsError(try NativeRuntimeControlCodec.validate(response, for: request))
        }
        let complete = Fixture.response(request, observation: Fixture.observation(request, phase: .completed, proof: true, exit: true))
        try NativeRuntimeControlCodec.validate(complete, for: request)
        XCTAssertThrowsError(try NativeRuntimeControlCodec.validate(complete, for: Fixture.request()))
        let mismatched = Fixture.request(target: .init(appInstanceID: Fixture.app, runToken: UUID().uuidString,
            processID: 123, acceptedConfigurationDigest: Fixture.target.acceptedConfigurationDigest))
        XCTAssertThrowsError(try NativeRuntimeControlCodec.validate(complete.observation!, target: mismatched.target))
    }

    func testExistingMayReturnCanonicalOperationButStatusCannotChangeIt() throws {
        let request = Fixture.request(), canonical = UUID().uuidString
        try NativeRuntimeControlCodec.validate(Fixture.response(request,
            observation: Fixture.observation(request, id: canonical), disposition: .existing), for: request)
        let status = Fixture.request(.stopStatus, id: request.operationID)
        XCTAssertThrowsError(try NativeRuntimeControlCodec.validate(Fixture.response(status,
            observation: Fixture.observation(status, id: canonical)), for: status))
    }

    func testImpossibleCleanupSummariesCannotBecomeCompletion() {
        let request = Fixture.request()
        for kind in ["missingReap", "swtpmCount", "notAdmitted", "removeFailed", "inventedFailure", "failedCleanup", "wrongOperation"] {
            let proof = NativeRuntimeCleanupObservation(operationID: kind == "wrongOperation" ? UUID().uuidString : nil,
                helperSpawnedCount: 1, helperReapedCount: kind == "missingReap" ? 0 : 1,
                swtpmSpawnedCount: kind == "swtpmCount" ? 2 : 0, swtpmReapedCount: kind == "swtpmCount" ? 2 : 0,
                mediaLeaseDisposition: kind == "notAdmitted" ? "notAdmitted" : "releasedAfterReap",
                runtimeDirectoryDisposition: kind == "removeFailed" ? "removeFailed" : "removed",
                failureCode: kind == "inventedFailure" ? "invented" : kind == "failedCleanup" ? "helperFailed" : nil)
            let valid = Fixture.observation(request, phase: .completed, proof: true, exit: true)
            let impossible = NativeRuntimeStopObservation(operationID: valid.operationID, phase: .completed,
                acceptedUptime: valid.acceptedUptime, deadlineUptime: valid.deadlineUptime,
                supervisorComplete: proof, runnerExit: valid.runnerExit, failure: nil)
            XCTAssertThrowsError(try NativeRuntimeControlCodec.validate(impossible, target: request.target), kind)
        }
    }

    func testPendingFinalOwnershipProofCanAlreadyContainRunnerExit() throws {
        let request = Fixture.request()
        let pending = Fixture.response(request, observation: Fixture.observation(request,
            phase: .awaitingRunnerExit, proof: true, exit: true))
        try NativeRuntimeControlCodec.validate(pending, for: request)
        XCTAssertNotEqual(pending.observation?.phase, .completed)
    }
}
