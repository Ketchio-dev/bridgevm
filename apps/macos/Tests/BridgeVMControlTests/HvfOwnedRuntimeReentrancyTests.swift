import Combine
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfOwnedRuntimeReentrancyTests: XCTestCase {
    func testLaunchEventResetCannotAdmitAnotherLaunchOrMutateAcceptedConfig() async throws {
        let f = try HvfOwnedRuntimeFixture()
        defer { f.clean() }
        f.waitScript()
        let session = f.session()
        var attempted: HvfRuntimeStartOutcome?
        var configurationAccepted: Bool?
        var activeDuringReset = false
        let observation = session.$events.dropFirst().sink { _ in
            guard attempted == nil else { return }
            attempted = session.start(policy: .requireNew)
            configurationAccepted = session.acceptStartConfiguration(f.config)
            activeDuringReset = session.hasActiveRuntimeWork
        }
        defer { observation.cancel() }
        guard case .ownedLaunchAccepted = session.start(policy: .requireNew) else {
            return XCTFail("Outer admitted launch must not refuse itself")
        }
        guard case .refused = attempted else { return XCTFail("Nested start must be refused before effects") }
        XCTAssertEqual(configurationAccepted, false)
        XCTAssertTrue(activeDuringReset)
        XCTAssertEqual(f.launches, 1)
        try f.releaseChild(); try await f.observeExit(session)
    }

    func testAdmissionRefusalCallbackCannotReenterAdmissionOrAcceptNewConfiguration() throws {
        let f = try HvfOwnedRuntimeFixture()
        defer { f.clean() }
        let session = f.session()
        var checks = 0
        var nested: HvfRuntimeStartOutcome?
        session.workAdmission = { _ in
            checks += 1
            XCTAssertFalse(session.hasActiveRuntimeWork, "Own admission must not see itself as already accepted")
            nested = session.start(policy: .requireNew)
            XCTAssertFalse(session.acceptStartConfiguration(f.config))
            return "reserved"
        }
        XCTAssertEqual(session.start(policy: .requireNew), .refused("reserved"))
        guard case .refused = nested else { return XCTFail("Nested admission must be rejected") }
        XCTAssertEqual(checks, 1)
        XCTAssertEqual(f.launches, 0)
        XCTAssertFalse(session.hasActiveRuntimeWork)
    }

    func testStoppedPublicationCannotReenterLaunchBeforeOldTransitionFinishes() async throws {
        let f = try HvfOwnedRuntimeFixture()
        defer { f.clean() }
        f.waitScript()
        let session = f.session()
        guard case .ownedLaunchAccepted = session.start(policy: .requireNew) else {
            return XCTFail("Expected retained harmless child")
        }
        var attempted: HvfRuntimeStartOutcome?
        let observation = session.$connectionState.dropFirst().sink { value in
            guard value == .stopped, attempted == nil else { return }
            attempted = session.start(policy: .requireNew)
        }
        defer { observation.cancel() }
        try f.releaseChild(); try await f.observeExit(session)
        guard case .refused = attempted else { return XCTFail("Synchronous willSet cannot admit a replacement run") }
        XCTAssertEqual(f.launches, 1)
        XCTAssertEqual(session.connectionState, .stopped)
        XCTAssertNil(session.ownedProcessIdentity)
        XCTAssertFalse(session.hasActiveRuntimeWork)
    }
}
