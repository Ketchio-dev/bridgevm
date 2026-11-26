import XCTest
@testable import BridgeVMControl

final class HvfGraphicsRuntimePresentationTests: XCTestCase {
    func testMissingPolicyIsBasicForNextLaunch() {
        let value = HvfGraphicsRuntimePresentation.classify(
            running: false, ownedLaunchRunning: false, experimental3DAllowed: nil)
        XCTAssertEqual(value, .nextLaunchBasic)
        XCTAssertEqual(value.label, "Next start · Basic display · 3D off")
    }

    func testExternalRunningSessionDoesNotInheritSavedPolicy() {
        for savedPolicy in [nil, false, true] as [Bool?] {
            let value = HvfGraphicsRuntimePresentation.classify(
                running: true, ownedLaunchRunning: false, experimental3DAllowed: savedPolicy)
            XCTAssertEqual(value, .runningUnverified)
            XCTAssertEqual(value.label, "Current session · Graphics mode unknown")
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
        XCTAssertEqual(value.label, "Next start · Graphics Lab · Experimental 3D")
    }
}
