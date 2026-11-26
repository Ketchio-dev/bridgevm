import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallSourceActivityTests: XCTestCase {
    func testSourceActivityTracksTheBuilderAndClearsForRetry() {
        let session = HvfWindowsInstallSession(plan: HvfWindowsInstallTestSupport.plan())
        session.transition(to: .preparingSource)
        session.appendLog("[build-src] copying ISO tree (excluding install.wim)")
        XCTAssertEqual(session.sourceActivity, "Copying setup files from the ISO…")
        session.appendLog("[build-src] splitting install.wim -> install.swm/install*.swm (<3800MB each)")
        for index in 0...400 { session.appendLog("WIM output \(index)") }
        XCTAssertEqual(session.logLines.count, 400)
        XCTAssertEqual(session.sourceActivity, "Splitting the Windows installation image…")
        session.appendLog("[build-src] verifying payload + boot files")
        XCTAssertEqual(session.sourceActivity, "Verifying installation files…")

        session.prepareAttemptPresentation()
        session.appendLog("[build-src] copying ISO tree (excluding install.wim)")
        XCTAssertNil(session.sourceActivity)
    }
}
