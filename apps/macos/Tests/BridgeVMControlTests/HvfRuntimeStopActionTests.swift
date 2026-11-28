import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeStopActionTests: XCTestCase {
    func testOwnedStopForwardsExactIdentityAndRefreshes() throws {
        let fixture = try HvfOwnedRuntimeFixture(readyForLaunch: false)
        defer { fixture.clean() }
        let session = fixture.session()
        let identity = HvfOwnedRuntimeIdentity(token: UUID(), processID: 42)
        session.ownedProcessIdentity = identity
        var forwarded: HvfOwnedRuntimeIdentity?
        var message: String?
        var refreshes = 0

        HvfRuntimeStopAction.perform(
            session: session,
            requestStop: { _, target in forwarded = target; return .requested(deadline: nil) },
            report: { message = $0 },
            refresh: { refreshes += 1 })

        XCTAssertEqual(forwarded, identity)
        XCTAssertEqual(message, "Stop requested.")
        XCTAssertEqual(refreshes, 1)
    }

    func testExternalAttachmentIsRefusedWithoutCallingStop() throws {
        let fixture = try HvfOwnedRuntimeFixture(readyForLaunch: false)
        defer { fixture.clean() }
        let session = fixture.session()
        session.attachedToExistingProcess = true
        var stopCalls = 0
        var message: String?

        HvfRuntimeStopAction.perform(
            session: session,
            requestStop: { _, _ in stopCalls += 1; return .requested(deadline: nil) },
            report: { message = $0 },
            refresh: { XCTFail("Refusal must not refresh after a stop") })

        XCTAssertEqual(stopCalls, 0)
        XCTAssertEqual(message, "Stop the VM from the app or tool that started it.")
    }

    func testPendingStartIsRefusedWithoutCallingStop() throws {
        let fixture = try HvfOwnedRuntimeFixture(readyForLaunch: false)
        defer { fixture.clean() }
        let session = fixture.session()
        session.guiStartOperation = HvfGUIStartOperation(configuration: fixture.config)
        var stopCalls = 0
        var message: String?

        HvfRuntimeStopAction.perform(
            session: session,
            requestStop: { _, _ in stopCalls += 1; return .requested(deadline: nil) },
            report: { message = $0 },
            refresh: { XCTFail("Refusal must not refresh after a stop") })

        XCTAssertEqual(stopCalls, 0)
        XCTAssertEqual(message, "Wait for startup to finish, then shut down the VM.")
    }

    func testOwnershipRaceReportsNotOwnedForCapturedIdentity() throws {
        let fixture = try HvfOwnedRuntimeFixture(readyForLaunch: false)
        defer { fixture.clean() }
        let session = fixture.session()
        let accepted = HvfOwnedRuntimeIdentity(token: UUID(), processID: 7)
        session.ownedProcessIdentity = accepted
        var forwarded: HvfOwnedRuntimeIdentity?
        var message: String?

        HvfRuntimeStopAction.perform(
            session: session,
            requestStop: { session, target in
                forwarded = target
                session.ownedProcessIdentity = HvfOwnedRuntimeIdentity(token: UUID(), processID: 8)
                return session.stopOwned(expectedToken: target.token)
            },
            report: { message = $0 },
            refresh: {})

        XCTAssertEqual(forwarded, accepted)
        XCTAssertEqual(message, "The running process changed. Check its status before trying to stop it again.")
    }
}
