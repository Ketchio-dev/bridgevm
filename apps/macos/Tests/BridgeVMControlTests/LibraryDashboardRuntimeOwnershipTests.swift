import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryDashboardRuntimeOwnershipTests: XCTestCase {
    func testRetainedSessionPresentsWithoutStartingOrAttachingAgain() throws {
        let session = try fixture(running: true); XCTAssertTrue(session.attachIfStopped())
        var presentations = 0
        LibraryDashboardPrimaryAction.perform(observedRunning: true, session: session,
            requestStart: { _ in XCTFail("started duplicate"); return .refused("duplicate") },
            attach: { _ in XCTFail("reattached active session"); return false },
            present: { _ in presentations += 1 }, report: { XCTFail($0) })
        XCTAssertEqual(presentations, 1)
    }

    func testMissingOrUnattachableSessionReportsInsteadOfPretendingToOpen() throws {
        var messages: [String] = []
        LibraryDashboardPrimaryAction.perform(observedRunning: false, session: nil,
            requestStart: { _ in XCTFail("started"); return .refused("unexpected") },
            attach: { _ in XCTFail("attached"); return false },
            present: { _ in XCTFail("presented") }, report: { messages.append($0) })
        let session = try fixture(running: false)
        LibraryDashboardPrimaryAction.perform(observedRunning: true, session: session,
            requestStart: { _ in XCTFail("started duplicate"); return .refused("duplicate") },
            attach: { _ in false }, present: { _ in XCTFail("presented") },
            report: { messages.append($0) })
        XCTAssertEqual(messages, ["VM 런타임 세션을 준비하지 못했습니다.",
                                  "실행 중인 VM 화면에 연결하지 못했습니다."])
    }

    func testStopTargetsExactOwnedIdentityAndRefreshes() throws {
        let session = try fixture(running: false)
        let identity = HvfOwnedRuntimeIdentity(token: UUID(), processID: 42)
        session.ownedProcessIdentity = identity
        var requested: HvfOwnedRuntimeIdentity?, messages: [String] = []; var refreshes = 0
        LibraryDashboardPrimaryAction.stop(session: session,
            requestStop: { _, target in requested = target; return .requested(deadline: nil) },
            report: { messages.append($0) }, refresh: { refreshes += 1 })
        XCTAssertEqual(requested, identity); XCTAssertEqual(messages, ["VM 정지 요청을 보냈습니다."])
        XCTAssertEqual(refreshes, 1)
    }

    func testStopRefusesObservedExternalRuntimeWithoutCallingStop() throws {
        let session = try fixture(running: true); XCTAssertTrue(session.attachIfStopped())
        var messages: [String] = []; var refreshes = 0
        LibraryDashboardPrimaryAction.stop(session: session,
            requestStop: { _, _ in XCTFail("stopped external runtime"); return .notOwned },
            report: { messages.append($0) }, refresh: { refreshes += 1 })
        XCTAssertEqual(messages, ["다른 실행 경로에서 시작된 VM은 이 앱이 안전하게 정지할 수 없습니다."])
        XCTAssertEqual(refreshes, 0)
    }

    private func fixture(running: Bool) throws -> HvfEngineSession {
        let fixture = try HvfOwnedRuntimeFixture(readyForLaunch: false); fixture.existingRuntime = running
        return fixture.session()
    }
}
