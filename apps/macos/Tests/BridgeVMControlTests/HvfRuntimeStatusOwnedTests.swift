import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeStatusOwnedTests: XCTestCase {
    func testOwnedObservationReadsRetainedConnectionWithoutConsumingLogs() throws {
        let f = try HvfRuntimeStatusOwnedFixture()
        defer { f.owned.clean() }
        f.owned.waitScript()
        guard case let .ownedLaunchAccepted(identity) = f.session.start(policy: .requireNew) else {
            return XCTFail("Expected a harmless owned child")
        }
        let log = URL(fileURLWithPath: f.owned.config.evidenceDir).appendingPathComponent("run.log")
        let unread = Data("BVAGENT READY host=never-read-by-status t=1\n".utf8)
        try unread.write(to: log)
        let effects = f.effects, events = f.session.events, heartbeat = f.session.lastHeartbeatAge
        for (state, name) in [(HvfConnectionState.booting, "booting"), (.connected(host: "private-fixture-host"), "connected"),
                              (.stopping, "stopping"), (.timedOut, "timedOut"), (.stopped, "stopped")] {
            f.session.connectionState = state
            let value = try f.status()
            XCTAssertEqual(value.ownership, .owned)
            XCTAssertEqual(value.connectionState, name)
            XCTAssertEqual(value.ownedProcess?.token, identity.token.uuidString)
            XCTAssertEqual(value.ownedProcess?.processID, identity.processID)
            XCTAssertNil(value.lastOwnedExit); XCTAssertEqual(value.graphicsMode, .basic3DOff)
            XCTAssertFalse(String(decoding: try NativeRuntimeCodec.encode(value), as: UTF8.self).contains("private-fixture-host"))
        }
        XCTAssertEqual(f.effects, effects)
        XCTAssertEqual(f.session.events, events)
        XCTAssertEqual(f.session.lastHeartbeatAge, heartbeat)
        XCTAssertEqual(try Data(contentsOf: log), unread)
        f.session.connectionState = .booting
        try f.owned.releaseChild()
        try f.waitForChildExit()
        f.session.poll()
        XCTAssertEqual(f.session.lastOwnedExit?.status, 0)
    }

    func testExitedChildRemainsOwnedUntilSessionExplicitlyObservesItsExit() throws {
        let f = try HvfRuntimeStatusOwnedFixture()
        defer { f.owned.clean() }
        f.owned.waitScript(exit: 23)
        guard case let .ownedLaunchAccepted(identity) = f.session.start(policy: .requireNew) else {
            return XCTFail("Expected a harmless owned child")
        }
        try f.owned.releaseChild()
        try f.waitForChildExit()
        let effects = f.effects, before = f.session.runtimeObservation()
        let unpolled = try f.status()
        XCTAssertEqual(unpolled.ownership, .owned, "Status must not refresh liveness by polling the Process")
        XCTAssertNil(unpolled.lastOwnedExit)
        XCTAssertEqual(f.session.runtimeObservation(), before)
        XCTAssertEqual(f.effects, effects)
        f.session.poll() // The existing runtime lifecycle, never the status query, observes the exit.
        let terminal = try f.status()
        XCTAssertEqual(terminal.ownership, .ownedExitObserved)
        XCTAssertEqual(terminal.connectionState, "stopped")
        XCTAssertNil(terminal.ownedProcess)
        XCTAssertEqual(terminal.lastOwnedExit?.process.token, identity.token.uuidString)
        XCTAssertEqual(terminal.lastOwnedExit?.process.processID, identity.processID)
        XCTAssertEqual(terminal.lastOwnedExit?.reason, "exit")
        XCTAssertEqual(terminal.lastOwnedExit?.status, 23); XCTAssertNil(terminal.graphicsMode)
        XCTAssertEqual(f.effects, effects)
    }

    func testAttachedObservationDoesNotClaimOwnershipOrProbeAgain() throws {
        let f = try HvfRuntimeStatusOwnedFixture(readyForLaunch: false)
        defer { f.owned.clean() }
        f.owned.existingRuntime = true
        XCTAssertEqual(f.session.start(), .observedAttachment)
        let effects = f.effects, before = f.session.runtimeObservation()
        let value = try f.status()
        XCTAssertEqual(value.ownership, .attachedObservation)
        XCTAssertEqual(value.connectionState, "booting")
        XCTAssertNil(value.ownedProcess)
        XCTAssertNil(value.lastOwnedExit); XCTAssertEqual(value.graphicsMode, .unverified)
        XCTAssertEqual(f.effects, effects)
        XCTAssertEqual(f.session.runtimeObservation(), before)
        XCTAssertEqual(f.owned.launches, 0)
        XCTAssertEqual(f.owned.keys.requests, 0)
    }

    func testAttachedObservationKeepsPriorOwnedSignalExitAsHistoryOnly() throws {
        let f = try HvfRuntimeStatusOwnedFixture()
        defer { f.owned.clean() }
        f.owned.waitScript()
        guard case let .ownedLaunchAccepted(identity) = f.session.start(policy: .requireNew) else {
            return XCTFail("Expected a harmless owned child")
        }
        let child = try XCTUnwrap(f.owned.child)
        child.terminate()
        try f.waitForChildExit()
        f.session.poll()
        let exited = try f.status()
        XCTAssertEqual(exited.ownership, .ownedExitObserved)
        XCTAssertEqual(exited.lastOwnedExit?.reason, "uncaughtSignal")
        XCTAssertEqual(exited.lastOwnedExit?.status, SIGTERM)
        f.owned.existingRuntime = true
        XCTAssertTrue(f.session.attachToRunningVM())
        let effects = f.effects
        let attached = try f.status()
        XCTAssertEqual(attached.ownership, .attachedObservation)
        XCTAssertNil(attached.ownedProcess)
        XCTAssertEqual(attached.lastOwnedExit, exited.lastOwnedExit)
        XCTAssertEqual(attached.lastOwnedExit?.process.token, identity.token.uuidString); XCTAssertEqual(attached.graphicsMode, .unverified)
        XCTAssertEqual(f.effects, effects)
    }
}

@MainActor
private final class HvfRuntimeStatusOwnedFixture {
    let owned: HvfOwnedRuntimeFixture
    let config: VMConfig
    let session: HvfEngineSession
    let library: LibraryModel
    var effects: [Int] { [owned.launches, owned.lookups, owned.keys.requests] }

    init(readyForLaunch: Bool = true) throws {
        let owned = try HvfOwnedRuntimeFixture(readyForLaunch: readyForLaunch)
        self.owned = owned
        let root = owned.root.appendingPathComponent("library")
        config = VMConfig(id: "status-owned", name: "owned", displayName: "owned", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: root.appendingPathComponent("owned.bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: "owned", displayWidth: 1280, displayHeight: 720, installPending: false)
        let session = owned.session()
        self.session = session
        library = LibraryModel(rootURL: root, migrateLegacy: false,
            runtimeSessionFactory: { _ in session }, startsModelsAutomatically: false)
        XCTAssertTrue(library.hvfRuntimeSessions.session(for: config, libraryRoot: root) === session)
    }

    func status() throws -> NativeRuntimeSessionObservation {
        let digest = try NativeRuntimeConfigurationIdentity.digest(config: config)
        let values = try library.runtimeObservations(slug: config.slug, requestedConfigurationIdentity: digest)
        XCTAssertEqual(values.count, 1)
        let value = try XCTUnwrap(values.first)
        try NativeRuntimeCodec.validate(value, saved: .init(state: .present, digest: digest))
        return value
    }

    func waitForChildExit() throws {
        let child = try XCTUnwrap(owned.child)
        let deadline = ProcessInfo.processInfo.systemUptime + 3
        // Keep the main actor occupied so the existing runtime timer cannot observe this exit first.
        while child.isRunning, ProcessInfo.processInfo.systemUptime < deadline {
            Thread.sleep(forTimeInterval: 0.005)
        }
        XCTAssertFalse(child.isRunning, "The exact harmless fixture child must exit within its deadline")
    }
}
