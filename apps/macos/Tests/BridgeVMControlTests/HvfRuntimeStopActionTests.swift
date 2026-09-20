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
        XCTAssertEqual(message, "VM 정지 요청을 보냈습니다.")
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
        XCTAssertEqual(message, "다른 실행 경로에서 시작된 VM은 이 앱이 안전하게 정지할 수 없습니다.")
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
        XCTAssertEqual(message, "VM 시작 준비가 끝난 뒤 다시 정지하세요.")
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
        XCTAssertEqual(message, "실행 프로세스 소유권이 변경되어 정지를 중단했습니다.")
    }
}
