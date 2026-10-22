import Foundation
import XCTest
@testable import BridgeVMControl

/// Each owned-runtime controller tick re-derives the connection state. Reassigning an unchanged
/// value republished every session observer, and through `$connectionState` the library model,
/// on every poll for as long as a stop stayed pending or unconfirmed.
@MainActor
final class HvfEngineSessionOwnedTickPublishTests: XCTestCase {
    func testOwnedStopAndFailureTicksDoNotRepublishAnUnchangedState() throws {
        let polls = HvfEngineSessionPublishTests.polls
        let f = try HvfOwnedRuntimeControllerTests.Fixture(); defer { f.clean() }
        try FileManager.default.createDirectory(atPath: f.base.config.evidenceDir, withIntermediateDirectories: true)
        let session = f.base.session(), token = f.controller.identity.token
        session.ownedController = f.controller
        f.controller.onChange = { [weak session] in session?.ownedRuntimeChanged(token: token) }
        session.connectionState = .connected(host: "t")
        let stop = HvfEngineSessionPublishTests.count(session) {
            guard case .accepted = session.requestOwnedStop(target: f.controller.identity, operationID: UUID()) else {
                return XCTFail("Expected an owned stop admission")
            }
        }
        XCTAssertEqual(session.connectionState, .stopping)
        let stopping = HvfEngineSessionPublishTests.count(session) { for _ in 0..<polls { session.poll() } }
        f.clock.value += HvfOwnedRunController.completionObservation // the pre-ready cancellation deadline passes
        let failure = HvfEngineSessionPublishTests.count(session) { session.poll() }
        XCTAssertEqual(session.connectionState, .timedOut)
        let timedOut = HvfEngineSessionPublishTests.count(session) { for _ in 0..<polls { session.poll() } }
        print("session-publish: owned polls=\(polls) stopping session=\(stopping.session) connectionState=\(stopping.state)"
            + "; timedOut session=\(timedOut.session) connectionState=\(timedOut.state); stop=\(stop) failure=\(failure)")
        for (name, change) in [("stop request", stop), ("unconfirmed stop", failure)] {
            XCTAssertGreaterThan(change.session, 0, name); XCTAssertGreaterThan(change.state, 0, name)
        }
        XCTAssertEqual(stopping.session, 0, "polls during an unchanged stop must not invalidate session observers")
        XCTAssertEqual(stopping.state, 0)
        XCTAssertEqual(timedOut.session, 0, "polls after an unchanged failure must not invalidate session observers")
        XCTAssertEqual(timedOut.state, 0)
        XCTAssertEqual(session.connectionState, .timedOut)
    }
}
