import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryDashboardPrimaryActionTests: XCTestCase {
    func testStoppedVMRequestsTypedStartAndPresentsAcceptedSession() throws {
        let session = try fixture(running: false)
        var starts = 0, presentations = 0, messages: [String] = []
        LibraryDashboardPrimaryAction.perform(observedRunning: false, session: session,
            requestStart: { value in
                XCTAssertTrue(value === session); starts += 1
                return .accepted(HvfGUIStartOperation(configuration: value.config))
            }, attach: { _ in XCTFail("attached stopped VM"); return false },
            present: { _ in presentations += 1 }, report: { messages.append($0) })
        XCTAssertEqual(starts, 1); XCTAssertEqual(presentations, 1)
        XCTAssertEqual(messages, ["VM 시작 중…"])
    }

    func testStoppedVMReportsTypedStartRefusalWithoutPresenting() throws {
        let session = try fixture(running: false); var messages: [String] = []
        LibraryDashboardPrimaryAction.perform(observedRunning: false, session: session,
            requestStart: { _ in .refused("saved configuration changed") },
            attach: { _ in XCTFail("attached stopped VM"); return false },
            present: { _ in XCTFail("presented refused start") }, report: { messages.append($0) })
        XCTAssertEqual(messages, ["saved configuration changed"])
    }

    func testRunningVMAdoptsProcessBeforePresenting() throws {
        let session = try fixture(running: true); var presented: HvfEngineSession?
        LibraryDashboardPrimaryAction.perform(observedRunning: true, session: session,
            requestStart: { _ in XCTFail("started duplicate"); return .refused("duplicate") },
            attach: { $0.attachIfStopped() }, present: { presented = $0 }, report: { XCTFail($0) })
        XCTAssertTrue(presented === session); XCTAssertTrue(session.hasRetainedAttachment)
    }

    private func fixture(running: Bool) throws -> HvfEngineSession {
        let fixture = try HvfOwnedRuntimeFixture(readyForLaunch: false); fixture.existingRuntime = running
        return fixture.session()
    }
}
