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
    }

    func testAdmissionAndReadyAloneDoNotFinishStartup() {
        let initial = Fixture.request()
        var time = 0.0, requests: [NativeRuntimeStartRequest] = []
        let result = NativeCLIStartWaiter.wait(request: initial, now: { time }, pause: { time += $0 }) { request in
            requests.append(request)
            let observation: NativeRuntimeStartObservation
            switch requests.count {
            case 1: observation = Fixture.observation(request)
            case 2: observation = Fixture.observation(request, phase: .awaitingStartup, target: Fixture.target)
            default: observation = Self.started(request)
            }
            return Fixture.response(request, observation: observation)
        }
        XCTAssertTrue(result.started)
        XCTAssertEqual(requests.map(\.operation), [.start, .startStatus, .startStatus])
        XCTAssertTrue(requests.allSatisfy { $0.operationID == initial.operationID && $0.appInstanceID == initial.appInstanceID && $0.library == initial.library })
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

    /// The start path's own sentence (with the real error text) is what the CLI prints.
    func testFailureAndRefusalPrintTheAppSentence() {
        let text = "BridgeVM couldn’t start the VM process, so quit and reopen BridgeVM and try again.\nThe file “hvf-runner” couldn’t be opened."
        for refused in [false, true] {
            var time = 0.0
            let result = NativeCLIStartWaiter.wait(request: Fixture.request(), now: { time }, pause: { time += $0 }) { request in
                refused ? Fixture.response(request, refusal: .refused, reason: text)
                    : Fixture.response(request, observation: Fixture.observation(request, phase: .failed, failure: text))
            }
            XCTAssertFalse(result.started)
            XCTAssertEqual(result.failure, text)
            let output = NativeCLIRuntimeStart(vmID: "vm", libraryPath: "/l", appInstanceID: nil, operationID: "op",
                started: false, observation: result.response?.observation, unavailableReason: result.failure)
            XCTAssertTrue(output.text.contains("Start result unavailable: " + text))
        }
    }

    /// The most common failure, BridgeVM not open, printed only the code `ownerUnavailable`.
    func testCommonFailureCodesPrintASentenceAndJSONKeepsTheCode() throws {
        let output = NativeCLIRuntimeStart(vmID: "vm", libraryPath: "/l", appInstanceID: nil, operationID: "op",
            started: false, observation: nil, unavailableReason: "ownerUnavailable")
        XCTAssertTrue(output.text.contains(
            "Start result unavailable: BridgeVM isn’t open for this library. Open BridgeVM, then try again."), output.text)
        XCTAssertTrue(String(decoding: try JSONEncoder().encode(output), as: UTF8.self).contains("ownerUnavailable"))
        XCTAssertEqual(NativeCLIReason.sentence("deadlineExceeded", vmID: "vm", action: "starting"),
                       "BridgeVM is still starting the VM. Run `bridgevm app status vm` to follow it.")
    }

    func testPendingLateFailureAndOwnerLossCannotSucceed() {
        for variant in ["pending", "late", "failed", "ownerLost"] {
            var time = 0.0
            let result = NativeCLIStartWaiter.wait(request: Fixture.request(), now: { time }, pause: { time += max(10, $0) }) { request in
                if variant == "ownerLost" { throw NativeRuntimeError.ownerUnavailable }
                if variant == "late" { time = 31; return Fixture.response(request, observation: Self.started(request)) }
                if variant == "failed" {
                    return Fixture.response(request, observation: Fixture.observation(request, phase: .failed, failure: "x"))
                }
                return Fixture.response(request)
            }
            XCTAssertFalse(result.started, variant)
            XCTAssertNotNil(result.failure, variant)
        }
    }

    private static func started(_ request: NativeRuntimeStartRequest) -> NativeRuntimeStartObservation {
        Fixture.observation(request, phase: .started, target: Fixture.target, proof: Fixture.proof())
    }
}
