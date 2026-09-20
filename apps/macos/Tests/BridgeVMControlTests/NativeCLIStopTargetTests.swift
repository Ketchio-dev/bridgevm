import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeCLIStopTargetTests: XCTestCase {
    private typealias Fixture = NativeRuntimeControlTestSupport

    func testHistoricOwnedExitCannotMaskCurrentAttachment() throws {
        let exit = NativeRuntimeExitObservation(process: .init(token: Fixture.token, processID: 123), reason: "exit", status: 1)
        let historic = NativeRuntimeSessionObservation(ownership: .ownedExitObserved, connectionState: "stopped",
            acceptedConfigurationDigest: Fixture.target.acceptedConfigurationDigest, configurationMatch: .unknown,
            ownedProcess: nil, lastOwnedExit: exit, graphicsMode: nil)
        let attached = NativeRuntimeSessionObservation(ownership: .attachedObservation, connectionState: "connected",
            acceptedConfigurationDigest: Fixture.target.acceptedConfigurationDigest, configurationMatch: .unknown,
            ownedProcess: nil, lastOwnedExit: exit, graphicsMode: .unverified)
        func response(_ sessions: [NativeRuntimeSessionObservation]) -> NativeRuntimeResponse {
            .init(schema: NativeRuntimeCodec.responseSchema, requestID: UUID().uuidString, library: Fixture.library,
                  vmID: "vm", appInstanceID: Fixture.app, observedAt: 1, scope: NativeRuntimeCodec.scope, sessions: sessions)
        }
        XCTAssertEqual(try NativeCLIRuntimeStop.selectTarget(response([historic])), Fixture.target)
        for sessions in [[attached], [historic, attached], [attached, historic]] {
            XCTAssertThrowsError(try NativeCLIRuntimeStop.selectTarget(response(sessions))) {
                XCTAssertEqual($0 as? NativeRuntimeStopRefusal, .unsupportedRuntime)
            }
        }
        XCTAssertThrowsError(try NativeCLIRuntimeStop.selectTarget(response([historic, historic])))
        XCTAssertThrowsError(try NativeCLIRuntimeStop.selectTarget(response([])))
    }
}
