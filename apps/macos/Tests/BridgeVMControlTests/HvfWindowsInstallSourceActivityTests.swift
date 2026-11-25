import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallSourceActivityTests: XCTestCase {
    func testSourceActivityTracksTheBuilderAndClearsForRetry() {
        let session = HvfWindowsInstallSession(plan: HvfWindowsInstallTestSupport.plan())
        session.transition(to: .preparingSource)
        session.appendLog("[build-src] copying ISO tree (excluding install.wim)")
        XCTAssertEqual(session.sourceActivity, "ISO에서 설치 파일을 복사하는 중…")
        session.appendLog("[build-src] splitting install.wim -> install.swm/install*.swm (<3800MB each)")
        for index in 0...400 { session.appendLog("WIM output \(index)") }
        XCTAssertEqual(session.logLines.count, 400)
        XCTAssertEqual(session.sourceActivity, "Windows 설치 이미지를 분할하는 중…")
        session.appendLog("[build-src] verifying payload + boot files")
        XCTAssertEqual(session.sourceActivity, "설치 파일을 확인하는 중…")

        session.prepareAttemptPresentation()
        session.appendLog("[build-src] copying ISO tree (excluding install.wim)")
        XCTAssertNil(session.sourceActivity)
    }
}
