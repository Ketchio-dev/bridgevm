import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfOwnedRuntimeRetentionTests: XCTestCase {
    func testMissingProtocolCompletionBlocksReplacementPruneAndNewStartAfterWrapperExit() async throws {
        let f = try HvfOwnedRuntimeFixture()
        defer { f.clean() }
        // Select the ordinary owned branch; injected launcher runs only a harmless shell.
        let runner = f.root.appendingPathComponent("target/release/hvf-runner")
        try Data("fixture marker\n".utf8).write(to: runner)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: runner.path)
        let session = f.session()
        var factories = 0
        let store = HvfRuntimeSessionStore { _ in factories += 1; return session }
        let config = VMConfig(id: "retained-owned", name: "retained", displayName: "retained", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: f.root.appendingPathComponent("owned.bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
            guestName: "owned", displayWidth: 1280, displayHeight: 720, installPending: false)
        XCTAssertTrue(store.session(for: config, libraryRoot: f.root) === session)
        guard case let .ownedLaunchAccepted(identity) = session.start(policy: .requireNew) else {
            return XCTFail("Expected harmless owned launch")
        }
        try await f.observeExit(session)
        XCTAssertEqual(session.lastOwnedExit?.identity, identity)
        XCTAssertTrue(session.mayHaveOwnedWork)
        session.connectionState = .stopped // Presentation cannot erase retained ownership uncertainty.
        XCTAssertFalse(session.acceptStartConfiguration(f.config))
        XCTAssertFalse(session.attachIfStopped())
        guard case .refused = session.start(policy: .requireNew) else { return XCTFail("Missing cleanup proof") }
        XCTAssertTrue(store.isActive(slug: config.slug))
        store.reconcile(with: [])
        XCTAssertTrue(store.existingRecord(slug: config.slug)?.session === session)
        let retained = store.retainedControls(excludingSlugs: [])
        XCTAssertEqual(retained.count, 1)
        XCTAssertTrue(try XCTUnwrap(retained.first).isActive)
        XCTAssertTrue(store.session(for: config, libraryRoot: f.root) === session)
        XCTAssertEqual(factories, 1)
        XCTAssertEqual(f.launches, 1)
    }
}
