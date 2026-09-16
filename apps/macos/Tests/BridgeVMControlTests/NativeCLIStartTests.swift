import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeCLIStartTests: XCTestCase {
    private typealias Fixture = NativeRuntimeStartTestSupport

    func testStartOptionsAndAbsentOwnerNeverCreateLibrary() throws {
        XCTAssertEqual(try NativeCLIOptions.parse(arguments: ["start", "개발-vm", "--json"]).command, .start("개발-vm"))
        XCTAssertTrue(try NativeCLIOptions.parse(arguments: ["start", "--help"]).showHelp)
        for args in [["start"], ["start", "../vm"], ["start", "vm", "extra"], ["start", "vm", "--timeout", "0"]] {
            XCTAssertThrowsError(try NativeCLIOptions.parse(arguments: args))
        }
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("start-absent-\(UUID().uuidString)")
        let result = NativeCLIRuntimeStart.run(rootURL: root, id: "vm")
        XCTAssertFalse(result.started)
        XCTAssertEqual(result.unavailableReason, "ownerUnavailable")
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.path))
        let keyFailure = NativeCLIRuntimeStart(vmID: "vm", libraryPath: root.path, appInstanceID: nil, operationID: "op", expectedSavedConfigurationDigest: nil, started: false, observation: nil, unavailableReason: "keyUnavailable")
        XCTAssertTrue(keyFailure.text.contains("Start this VM once in BridgeVMControl"))
    }

    func testAdmissionAndReadyAloneDoNotFinishStartup() {
        let initial = Fixture.request()
        var time = 0.0, requests: [NativeRuntimeStartRequest] = []
        let result = NativeCLIStartWaiter.wait(request: initial, now: { time }, pause: { time += $0 }) { request in
            requests.append(request)
            let observation: NativeRuntimeStartObservation
            switch requests.count {
            case 1: observation = Fixture.observation(request)
            case 2: observation = Fixture.observation(request, phase: .awaitingStartup, target: Fixture.target, worker: false, owned: true)
            default: observation = Self.started(request)
            }
            return Fixture.response(request, observation: observation)
        }
        XCTAssertTrue(result.started)
        XCTAssertEqual(requests.map(\.operation), [.start, .startStatus, .startStatus])
        XCTAssertTrue(requests.allSatisfy { $0.operationID == initial.operationID && $0.appInstanceID == initial.appInstanceID && $0.expectedSavedConfigurationDigest == initial.expectedSavedConfigurationDigest && $0.library == initial.library })
        XCTAssertEqual(Set(requests.map(\.requestID)).count, 3)
    }

    func testLostAdmissionProbesThenRetriesTheSameOperation() {
        let initial = Fixture.request()
        var time = 0.0, requests: [NativeRuntimeStartRequest] = []
        let result = NativeCLIStartWaiter.wait(request: initial, now: { time }, pause: { time += $0 }) { request in
            requests.append(request)
            if requests.count == 1 { throw NativeRuntimeError.transportFailure }
            if requests.count == 2 { return Fixture.response(request, refusal: .operationUnknown) }
            return Fixture.response(request, observation: Self.started(request))
        }
        XCTAssertTrue(result.started)
        XCTAssertEqual(requests.map(\.operation), [.start, .startStatus, .start])
        XCTAssertTrue(requests.allSatisfy { $0.operationID == initial.operationID })
    }

    func testKnownAdmissionIsNeverResubmittedAfterUnknownStatus() {
        var time = 0.0, operations: [NativeRuntimeStartRequest.Operation] = []
        let result = NativeCLIStartWaiter.wait(request: Fixture.request(), now: { time }, pause: { time += $0 }) { request in
            operations.append(request.operation)
            return operations.count == 1 ? Fixture.response(request) : Fixture.response(request, refusal: .operationUnknown)
        }
        XCTAssertFalse(result.started)
        XCTAssertEqual(result.failure, "operationUnknown")
        XCTAssertEqual(operations, [.start, .startStatus])
    }

    func testPendingLateFailureOwnerLossAndIncompleteProofCannotSucceed() {
        for variant in ["pending", "late", "failed", "unconfirmed", "ownerLost", "invalid"] {
            var time = 0.0
            let result = NativeCLIStartWaiter.wait(request: Fixture.request(), now: { time }, pause: { time += max(10, $0) }) { request in
                if variant == "ownerLost" { throw NativeRuntimeError.ownerUnavailable }
                if variant == "late" { time = 31; return Fixture.response(request, observation: Self.started(request)) }
                if variant == "failed" || variant == "unconfirmed" {
                    return Fixture.response(request, observation: Fixture.observation(request,
                        phase: variant == "failed" ? .failed : .unconfirmed, failure: .keyUnavailable,
                        worker: variant == "unconfirmed"))
                }
                if variant == "invalid" {
                    return Fixture.response(request, observation: Fixture.observation(request, phase: .started,
                        target: Fixture.target, worker: false, owned: true))
                }
                return Fixture.response(request)
            }
            XCTAssertFalse(result.started, variant)
            XCTAssertNotNil(result.failure, variant)
        }
    }

    private static func started(_ request: NativeRuntimeStartRequest) -> NativeRuntimeStartObservation {
        Fixture.observation(request, phase: .started, target: Fixture.target, proof: Fixture.proof(), worker: false, owned: true)
    }
}
