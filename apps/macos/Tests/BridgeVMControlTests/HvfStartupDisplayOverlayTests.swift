import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfStartupDisplayOverlayTests: XCTestCase {
    /// The start worker is done and the firmware runs, but no frame has come: the window says Windows is
    /// starting instead of staying plain black, until the display view shows the first frame.
    func testTheWindowSaysWindowsIsStartingUntilItsFirstFrame() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            session.connectionState = .booting
            defer { session.connectionState = .stopped }
            XCTAssertEqual(session.startupDisplaySpinnerText, "Starting Windows…")

            try HvfFramebufferAvailabilityFixture.writeFrame(fixture.evidence.appendingPathComponent("display.fb"))
            let view = FBLayerView(session: session, imageFactory: .init())
            view.frame = NSRect(x: 0, y: 0, width: 100, height: 100)
            defer { view.teardown() }
            view.refreshFrame(at: 0)
            XCTAssertNil(session.startupDisplaySpinnerText, "the first frame shows Windows itself")

            session.resetObservedRuntimeState(clearEvents: false)
            XCTAssertEqual(session.startupDisplaySpinnerText, "Starting Windows…", "a new run waits for its own frame")
        }
    }
}
