import Combine
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfOwnedRuntimeAttachmentReentrancyTests: XCTestCase {
    private let diagnostic = BvAgentEvent.unknown(
        "attached to the already running HVF engine; duplicate launch prevented")

    func testDuplicateLaunchDiagnosticIsPublishedInsideAttachmentFence() throws {
        let f = try HvfOwnedRuntimeFixture(readyForLaunch: false)
        defer { f.clean() }
        f.existingRuntime = true
        let session = f.session()
        var reentrant: HvfRuntimeStartOutcome?
        let observer = session.$events.dropFirst().sink { [diagnostic] events in
            guard events.contains(diagnostic), reentrant == nil else { return }
            reentrant = session.start()
        }
        defer { observer.cancel() }
        XCTAssertEqual(session.start(), .observedAttachment)
        guard case .refused = reentrant else { return XCTFail("Diagnostic observer cannot reenter attachment") }
        XCTAssertEqual(session.connectionState, .booting)
        XCTAssertTrue(session.events.contains(diagnostic))
        XCTAssertEqual(f.lookups, 1)
        XCTAssertEqual(f.launches, 0)
        XCTAssertEqual(f.keys.requests, 0)
        XCTAssertFalse(FileManager.default.fileExists(atPath: f.config.ctlFilePath))
    }

    func testExplicitAttachmentKeepsItsExistingDiagnosticBehavior() throws {
        let f = try HvfOwnedRuntimeFixture(readyForLaunch: false)
        defer { f.clean() }
        f.existingRuntime = true
        let session = f.session()
        XCTAssertTrue(session.attachToRunningVM())
        XCTAssertFalse(session.events.contains(diagnostic))
        XCTAssertEqual(f.lookups, 1)
        XCTAssertEqual(f.launches, 0)
    }
}
