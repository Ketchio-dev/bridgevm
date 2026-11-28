import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeCLIStopTests: XCTestCase {
    private typealias Fixture = NativeRuntimeControlTestSupport

    func testStopOptionsAndAbsentOwnerNeverCreateLibrary() throws {
        XCTAssertEqual(try NativeCLIOptions.parse(arguments: ["stop", "개발-vm", "--json"]).command, .stop("개발-vm"))
        XCTAssertTrue(try NativeCLIOptions.parse(arguments: ["stop", "--help"]).showHelp)
        for args in [["stop"], ["stop", "../vm"], ["stop", "vm", "extra"], ["stop", "vm", "--timeout", "0"]] {
            XCTAssertThrowsError(try NativeCLIOptions.parse(arguments: args))
        }
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("stop-absent-\(UUID().uuidString)")
        let result = NativeCLIRuntimeStop.run(rootURL: root, id: "vm")
        XCTAssertFalse(result.complete)
        XCTAssertEqual(result.unavailableReason, "ownerUnavailable")
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.path))
    }

    func testDefaultWaitRequiresBothProofsAndUsesCanonicalOperation() {
        let initial = Fixture.request(), canonical = UUID().uuidString
        var time = 0.0, requests: [NativeRuntimeControlRequest] = []
        let result = NativeCLIStopWaiter.wait(request: initial, now: { time }, pause: { time += $0 }) { request in
            requests.append(request)
            if requests.count == 1 {
                return Fixture.response(request, observation: Fixture.observation(request, id: canonical), disposition: .existing)
            }
            if requests.count == 2 {
                return Fixture.response(request, observation: Fixture.observation(request, phase: .awaitingRunnerExit, proof: true))
            }
            if requests.count == 3 {
                return Fixture.response(request, observation: Fixture.observation(request,
                    phase: .awaitingRunnerExit, proof: true, exit: true))
            }
            return Fixture.response(request, observation: Fixture.observation(request, phase: .completed, proof: true, exit: true))
        }
        XCTAssertTrue(result.complete)
        XCTAssertEqual(requests.map(\.operation), [.stop, .stopStatus, .stopStatus, .stopStatus])
        XCTAssertEqual(requests.dropFirst().map(\.operationID), [canonical, canonical, canonical])
        XCTAssertTrue(requests.allSatisfy { $0.target == initial.target })
        XCTAssertEqual(Set(requests.map(\.requestID)).count, 4)
    }

    func testLostSubmissionResponseProbesThenRetriesSameOperationOnly() {
        let initial = Fixture.request()
        var time = 0.0, requests: [NativeRuntimeControlRequest] = []
        let result = NativeCLIStopWaiter.wait(request: initial, now: { time }, pause: { time += $0 }) { request in
            requests.append(request)
            if requests.count == 1 { throw NativeRuntimeError.transportFailure }
            if requests.count == 2 { return Fixture.response(request, refusal: .operationUnknown) }
            return Fixture.response(request, observation: Fixture.observation(request, phase: .completed, proof: true, exit: true))
        }
        XCTAssertTrue(result.complete)
        XCTAssertEqual(requests.map(\.operation), [.stop, .stopStatus, .stop])
        XCTAssertTrue(requests.allSatisfy { $0.operationID == initial.operationID && $0.target == initial.target })
    }

    func testPendingTimeoutLateSuccessAndUnconfirmedNeverReturnSuccess() {
        for variant in ["pending", "late", "unconfirmed", "ownerLost"] {
            var time = 0.0
            let result = NativeCLIStopWaiter.wait(request: Fixture.request(), now: { time }, pause: { time += max(70, $0) }) { request in
                if variant == "ownerLost" { throw NativeRuntimeError.ownerUnavailable }
                if variant == "late" { time = 211 }
                let phase: NativeRuntimeStopObservation.Phase = variant == "unconfirmed" ? .unconfirmed : variant == "late" ? .completed : .guestGrace
                return Fixture.response(request, observation: Fixture.observation(request, phase: phase,
                    proof: variant == "late", exit: variant == "late"))
            }
            XCTAssertFalse(result.complete, variant)
            XCTAssertNotNil(result.failure, variant)
        }
    }

    /// Text names the result in plain words; the run token, PID, operation and scope stay in JSON.
    /// Every stop failure the app reports, its own missed deadline included, ends unconfirmed.
    func testStopTextIsPlainAndEveryUnconfirmedStopPointsToForceStop() throws {
        let request = Fixture.request()
        func stop(_ phase: NativeRuntimeStopObservation.Phase, _ reason: String?) -> NativeCLIRuntimeStop {
            NativeCLIRuntimeStop(vmID: "vm", libraryPath: "/l", target: Fixture.target, operationID: request.operationID,
                complete: reason == nil, observation: Fixture.observation(request, phase: phase, proof: true, exit: true),
                unavailableReason: reason)
        }
        let stopped = stop(.completed, nil)
        XCTAssertEqual(stopped.text, "Stopped vm.\n")
        let json = String(decoding: try JSONEncoder().encode(stopped), as: UTF8.self)
        for value in [Fixture.token, request.operationID, NativeRuntimeControlCodec.scope, "\"processID\":123"] {
            XCTAssertTrue(json.contains(value), value)
        }
        for failure in ["cleanupUnconfirmed", "channelFailed", "runnerExitedWithoutComplete", "deadlineExceeded"] {
            XCTAssertEqual(stop(.unconfirmed, failure).text, """
            Couldn’t confirm that vm stopped.
            Check the VM in BridgeVM, and choose Force Stop if it’s still running.

            """, failure)
        }
        XCTAssertEqual(stop(.guestGrace, "deadlineExceeded").text, """
        Couldn’t confirm that vm stopped.
        Status: Waiting for Windows to shut down
        BridgeVM is still stopping the VM. Run `bridgevm app status vm` to follow it.

        """)
    }
}
