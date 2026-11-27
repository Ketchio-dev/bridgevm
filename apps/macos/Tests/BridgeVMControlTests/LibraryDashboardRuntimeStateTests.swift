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
        XCTAssertNil(stopped.note("Starting VM…"))
        XCTAssertNil(stopped.note("Stop requested."))
        XCTAssertNil(stopped.note("The VM has already stopped."))
        XCTAssertEqual(stopped.note("Couldn’t open the display for the running VM."),
                       "Couldn’t open the display for the running VM.")
    }

    /// A stop that left the disk unsaved, or a stop that failed, is not a normal stop or a boot.
    func testUnsavedDiskAndFailedStopNeedAttention() throws {
        let fixture = try HvfOwnedRuntimeFixture(readyForLaunch: false)
        let session = fixture.session()
        session.connectionState = .stopped
        session.handle(.diskFinalSaveFailed)
        XCTAssertEqual(LibraryDashboardRuntimeState(session: session, observedRunning: false), .failed)
        XCTAssertEqual(session.runtimeFailureText,
            "BridgeVM couldn’t finish saving the Windows disk at shutdown. Check your Mac’s free space and drive connection.")
        session.resetObservedRuntimeState(clearEvents: true)
        session.connectionState = .timedOut
        XCTAssertEqual(LibraryDashboardRuntimeState(session: session, observedRunning: true), .failed)
    }
}
