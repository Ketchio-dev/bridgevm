import XCTest
@testable import BridgeVMControl

final class HvfGraphicsRuntimePresentationTests: XCTestCase {
    func testMissingPolicyIsBasicForNextLaunch() {
        let value = HvfGraphicsRuntimePresentation.classify(
            running: false, ownedLaunchRunning: false, experimental3DAllowed: nil)
        XCTAssertEqual(value, .nextLaunchBasic)
        XCTAssertEqual(value.label, "다음 시작: 기본 디스플레이 · 3D 제외")
    }

    func testExternalRunningSessionDoesNotInheritSavedPolicy() {
        for savedPolicy in [nil, false, true] as [Bool?] {
            let value = HvfGraphicsRuntimePresentation.classify(
                running: true, ownedLaunchRunning: false, experimental3DAllowed: savedPolicy)
            XCTAssertEqual(value, .runningUnverified)
            XCTAssertEqual(value.label, "실행 그래픽: 기존 세션 · 모드 확인 불가")
        }
    }

    func testOwnedRunningSessionReportsFrozenPolicy() {
        XCTAssertEqual(HvfGraphicsRuntimePresentation.classify(
            running: true, ownedLaunchRunning: true, experimental3DAllowed: nil), .runningBasic)
        XCTAssertEqual(HvfGraphicsRuntimePresentation.classify(
            running: true, ownedLaunchRunning: true, experimental3DAllowed: true), .runningExperimental)
    }

    func testStoppedExperimentalPolicyIsExplicitlyFuturePath() {
        let value = HvfGraphicsRuntimePresentation.classify(
            running: false, ownedLaunchRunning: false, experimental3DAllowed: true)
        XCTAssertEqual(value, .nextLaunchExperimental)
        XCTAssertEqual(value.label, "다음 시작: Graphics Lab · 실험적 3D")
    }
}
