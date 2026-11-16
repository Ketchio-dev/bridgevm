import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryDashboardRuntimeStateTests: XCTestCase {
    func testAcceptedStartAndCompletedStopOverrideStaleBackendObservation() throws {
        let fixture = try HvfOwnedRuntimeFixture(readyForLaunch: false)
        let session = fixture.session()
        let start = HvfGUIStartOperation(configuration: session.config)
        session.guiStartOperation = start
        XCTAssertEqual(LibraryDashboardRuntimeState(session: session, observedRunning: false), .starting)
        start.finish(.observedAttachment)
        session.connectionState = .connected(host: "fixture")
        XCTAssertEqual(LibraryDashboardRuntimeState(session: session, observedRunning: false), .running)
        session.connectionState = .stopped
        let stopped = LibraryDashboardRuntimeState(session: session, observedRunning: true)
        XCTAssertEqual(stopped, .stopped)
        XCTAssertNil(stopped.note("VM 시작 중…"))
        XCTAssertEqual(stopped.note("디스크에 접근할 수 없습니다."), "디스크에 접근할 수 없습니다.")
    }
}
