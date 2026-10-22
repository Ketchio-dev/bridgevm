import Combine
import Foundation
import XCTest
@testable import BridgeVMControl

/// Every view holding the session as an `@ObservedObject`, the live display surface included,
/// re-evaluates on each `objectWillChange`. A poll that changes nothing those views show must not
/// emit it; a real runtime state change must still emit it at once. `$connectionState` is counted
/// too: the library's retained-control store republishes the whole library model on each value.
@MainActor
final class HvfEngineSessionPublishTests: XCTestCase {
    static let polls = 20
    static let boot = "BVAGENT READY host=t t=1\nBVAGENT SERVICE start t=2\n"

    func testHeartbeatOnlyPollsDoNotRepublishTheSession() throws {
        let root = try Self.root(); defer { try? FileManager.default.removeItem(at: root) }
        let session = try Self.attached(root, log: Self.boot + "BVAGENT SERVICE alive t=3\n")
        let age = try XCTUnwrap(session.lastHeartbeatAge)
        let events = session.events, state = session.connectionState
        let polls = Self.count(session) { for _ in 0..<Self.polls { session.poll() } }
        print("session-publish: heartbeat-only polls=\(Self.polls) session=\(polls.session) connectionState=\(polls.state)"
            + " heartbeat=\(polls.heartbeat)")
        XCTAssertEqual(polls.session, 0, "heartbeat-only polls must not invalidate session observers")
        XCTAssertEqual(polls.state, 0)
        XCTAssertEqual(polls.heartbeat, Self.polls, "the heartbeat texts still update on every poll")
        let advanced = try XCTUnwrap(session.lastHeartbeatAge)
        XCTAssertGreaterThan(advanced, age, "every poll still advances the heartbeat age")
        XCTAssertEqual(session.events, events)
        XCTAssertEqual(session.connectionState, state)
        session.markStopped()
    }

    func testRuntimeStateChangesStillPublishTheSessionAtOnce() throws {
        let root = try Self.root(); defer { try? FileManager.default.removeItem(at: root) }
        let session = HvfEngineSession(config: Self.config(root), repoRoot: root) { _ in true }
        let attach = Self.count(session) { XCTAssertTrue(session.attachToRunningVM()) }
        session.timer?.invalidate(); session.timer = nil // the test drives every poll itself
        XCTAssertEqual(session.connectionState, .booting)
        try Self.append(Self.boot, root)
        let ready = Self.count(session) { session.poll() }
        XCTAssertEqual(session.connectionState, .connected(host: "t"))
        try Self.append("BVAGENT SERVICE alive t=3\n", root)
        let heartbeat = Self.count(session) { session.poll() }
        XCTAssertEqual(session.events.last, .aliveHeartbeat(tMs: 3))
        let stop = Self.count(session) { session.markStopped() }
        XCTAssertEqual(session.connectionState, .stopped)
        XCTAssertNil(session.lastHeartbeatAge)
        print("session-publish: transitions attach=\(attach) ready=\(ready) heartbeat-line=\(heartbeat) stop=\(stop)")
        for (name, change) in [("attach", attach), ("ready", ready), ("stop", stop)] {
            XCTAssertGreaterThan(change.session, 0, name); XCTAssertGreaterThan(change.state, 0, name)
        }
        XCTAssertGreaterThan(heartbeat.session, 0, "a heartbeat line is a new event in the feed")
        XCTAssertGreaterThan(heartbeat.heartbeat, 0, "a heartbeat line resets the shown age")
        XCTAssertGreaterThan(stop.heartbeat, 0, "stopping clears the shown age")
    }

    static func count(_ session: HvfEngineSession, _ body: () -> Void) -> (session: Int, state: Int, heartbeat: Int) {
        var changes = 0, states = 0, heartbeats = 0
        let observers = [session.objectWillChange.sink { changes += 1 },
                         session.$connectionState.dropFirst().sink { _ in states += 1 },
                         session.heartbeat.objectWillChange.sink { heartbeats += 1 }]
        body()
        observers.forEach { $0.cancel() }
        return (changes, states, heartbeats)
    }

    private static func attached(_ root: URL, log: String) throws -> HvfEngineSession {
        let session = HvfEngineSession(config: config(root), repoRoot: root) { _ in true }
        try append(log, root)
        XCTAssertTrue(session.attachToRunningVM())
        session.timer?.invalidate(); session.timer = nil // the test drives every poll itself
        return session
    }

    private static func root() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root
    }

    private static func config(_ root: URL) -> HvfEngineConfig {
        HvfEngineConfig(targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: true, virtioGpu3d: false,
            nvmeBufferedIO: true, ctlFilePath: root.appendingPathComponent("agent.ctl").path)
    }

    private static func append(_ text: String, _ root: URL) throws {
        let url = root.appendingPathComponent("run.log")
        if !FileManager.default.fileExists(atPath: url.path) { FileManager.default.createFile(atPath: url.path, contents: nil) }
        let handle = try FileHandle(forWritingTo: url)
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(text.utf8))
    }
}
