import AppKit
import XCTest
@testable import BridgeVMControl

/// Unplugging the drive with a running VM's disk left the app silent: Windows only got disk errors.
@MainActor
final class HvfDriveDisconnectTests: XCTestCase {
    func testUnmountingTheRunningVMsDriveSaysSoUntilTheNextStart() throws {
        let f = try HvfOwnedRuntimeControllerTests.Fixture()
        defer { f.clean() }
        let session = HvfEngineSession(config: f.base.config, repoRoot: f.base.root) { _ in false }
        let disk = URL(fileURLWithPath: f.base.config.targetDiskPath)
        let volume = disk.deletingLastPathComponent().deletingLastPathComponent()
        HvfDriveDisconnect.watch { [session] }
        func unmount(_ url: URL) {
            NSWorkspace.shared.notificationCenter.post(name: NSWorkspace.didUnmountNotification, object: nil,
                userInfo: [NSWorkspace.volumeURLUserInfoKey: url])
        }

        unmount(volume)
        XCTAssertNil(session.runtimeSupervisionFailureText, "a stopped VM has nothing to say")
        session.connectionState = .connected(host: "fixture")
        unmount(URL(fileURLWithPath: "/Volumes/Another Drive"))
        XCTAssertNil(session.runtimeSupervisionFailureText, "another drive")
        unmount(volume)
        XCTAssertEqual(session.runtimeSupervisionFailureText, HvfDriveDisconnect.text)
        // The VM window says it too, until a stop is under way (its own strip then says what is happening).
        XCTAssertEqual(session.displayRunFailureText, HvfDriveDisconnect.text)
        session.stopWait = .waitingForGuestAgent
        XCTAssertNil(session.displayRunFailureText)
        session.stopWait = nil
        session.resetObservedRuntimeState(clearEvents: true)
        XCTAssertNil(session.runtimeSupervisionFailureText, "a new run starts afresh")
        HvfDriveDisconnect.watch { [] }
    }
}
