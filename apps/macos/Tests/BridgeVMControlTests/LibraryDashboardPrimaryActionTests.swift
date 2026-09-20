import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryDashboardPrimaryActionTests: XCTestCase {
    func testStoppedVMRoutesToStartOnly() {
        var started = 0, presented = 0
        LibraryDashboardPrimaryAction.perform(running: false, start: { started += 1 },
            resolve: { XCTFail("resolved running session"); return nil },
            present: { _ in presented += 1 }, report: { XCTFail($0) })
        XCTAssertEqual(started, 1); XCTAssertEqual(presented, 0)
    }

    func testRunningVMAdoptsProcessBeforePresenting() throws {
        let session = try fixture(running: true)
        var presented: HvfEngineSession?
        LibraryDashboardPrimaryAction.perform(running: true, start: { XCTFail("started duplicate") },
            resolve: { session }, present: { presented = $0 }, report: { XCTFail($0) })
        XCTAssertTrue(presented === session); XCTAssertTrue(session.hasRetainedAttachment)
    }

    func testRetainedSessionPresentsWithoutNewAttachment() throws {
        let session = try fixture(running: true)
        XCTAssertTrue(session.attachIfStopped())
        var presentations = 0
        LibraryDashboardPrimaryAction.perform(running: true, start: { XCTFail("started duplicate") },
            resolve: { session }, present: { _ in presentations += 1 }, report: { XCTFail($0) })
        XCTAssertEqual(presentations, 1)
    }

    func testMissingOrUnattachableSessionReportsInsteadOfPretendingToOpen() throws {
        var messages: [String] = []
        LibraryDashboardPrimaryAction.perform(running: true, start: { XCTFail("started duplicate") },
            resolve: { nil }, present: { _ in XCTFail("presented") }, report: { messages.append($0) })
        let session = try fixture(running: false)
        LibraryDashboardPrimaryAction.perform(running: true, start: { XCTFail("started duplicate") },
            resolve: { session }, present: { _ in XCTFail("presented") },
            report: { messages.append($0) })
        XCTAssertEqual(messages, ["실행 중인 VM의 화면 세션을 찾지 못했습니다.",
                                  "실행 중인 VM 화면에 연결하지 못했습니다."])
    }

    private func fixture(running: Bool) throws -> HvfEngineSession {
        let fixture = try HvfOwnedRuntimeFixture(readyForLaunch: false); fixture.existingRuntime = running
        return fixture.session()
    }
}
