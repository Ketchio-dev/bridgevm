import XCTest
@testable import BridgeVMControl

final class HvfGraphicsRuntimePresentationTests: XCTestCase {
    func testMissingPolicyIsBasicForNextLaunch() {
        let value = HvfGraphicsRuntimePresentation.classify(running: false, ownedRun3D: nil, saved3D: nil)
        XCTAssertEqual(value, .nextLaunchBasic)
        XCTAssertEqual(value.label, "Next start · Basic display · 3D off")
    }

    func testExternalRunningSessionDoesNotInheritSavedPolicy() {
        for savedPolicy in [nil, false, true] as [Bool?] {
            let value = HvfGraphicsRuntimePresentation.classify(running: true, ownedRun3D: nil, saved3D: savedPolicy)
            XCTAssertEqual(value, .runningUnverified)
            XCTAssertEqual(value.label, "Current session · Graphics mode unknown")
        }
    }

    func testOwnedRunningSessionReportsItsOwnRunNotTheSavedPolicy() {
        XCTAssertEqual(HvfGraphicsRuntimePresentation.classify(running: true, ownedRun3D: false, saved3D: true), .runningBasic)
        XCTAssertEqual(HvfGraphicsRuntimePresentation.classify(running: true, ownedRun3D: true, saved3D: false), .runningExperimental)
    }

    func testStoppedExperimentalPolicyIsExplicitlyFuturePath() {
        let value = HvfGraphicsRuntimePresentation.classify(running: false, ownedRun3D: nil, saved3D: true)
        XCTAssertEqual(value, .nextLaunchExperimental)
        XCTAssertEqual(value.label, "Next start · Graphics Lab · Experimental 3D")
    }

    /// The dashboard said "Graphics mode unknown" for every Windows the app itself started: it
    /// asked a launch path the app no longer uses. The session's own run answers now.
    @MainActor
    func testTheSessionsOwnRunningProcessReportsItsGraphics() throws {
        let f = try HvfOwnedRuntimeControllerTests.Fixture(); defer { f.clean() }
        let session = f.base.session()
        XCTAssertNil(session.ownedRunExperimental3D, "no run of its own")
        session.process = f.process; session.ownedController = f.controller
        XCTAssertEqual(session.ownedRunExperimental3D, f.base.config.virtioGpu3d)
        XCTAssertEqual(HvfGraphicsRuntimePresentation.classify(running: true,
            ownedRun3D: session.ownedRunExperimental3D, saved3D: nil), .runningBasic)
    }
}
