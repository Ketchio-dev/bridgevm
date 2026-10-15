import Foundation
import XCTest
@testable import BridgeVMControl

/// A guest reset record ends the boot generation whose `BVAGENT SERVICE start` opened
/// the control gate. The next harness tails agent.ctl from its own start offset, so a
/// line appended before the next generation's service starts can be skipped.
@MainActor
final class HvfEngineSessionGuestResetGateTests: XCTestCase {
    static let boot = "BVAGENT READY host=t t=1\nBVAGENT SERVICE start t=2\n"
    static let resets = ["PSCI SYSTEM_RESET: reboot 1/8\nuserspace GIC reset\n",
                         HvfGuestResetRecord.processRecreation + "\n"]
    static let shutdown = "shutdown.exe /p /f\n"

    func testControlCommandsAfterResetAreRefusedUntilNextServiceStart() throws {
        for reset in Self.resets {
            let root = try Self.root(); defer { try? FileManager.default.removeItem(at: root) }
            let session = HvfEngineSession(config: Self.config(root), repoRoot: root) { _ in true }
            try Self.append(Self.boot, root)
            XCTAssertTrue(session.attachToRunningVM())
            XCTAssertTrue(session.sendCtl("hostname"), reset)
            try Self.append(reset, root); session.poll()
            XCTAssertFalse(session.sendCtl("whoami"), reset)
            XCTAssertEqual(session.events.last, .unknown("control command refused: guest service has not started"), reset)
            XCTAssertEqual(session.sendText("é"), .refused, "clipboard paste: \(reset)")
            XCTAssertFalse(HvfWindowInventoryController(session: session).refresh(), reset)
            try Self.append("BVAGENT READY host=t t=1\n", root); session.poll()
            XCTAssertFalse(session.sendCtl("whoami"), "READY alone does not start the service: \(reset)")
            XCTAssertEqual(try Self.control(root), "hostname\n", reset)
            try Self.append("BVAGENT SERVICE start t=2\n", root); session.poll()
            XCTAssertTrue(session.sendCtl("ver"), reset)
            XCTAssertEqual(try Self.control(root), "hostname\nver\n", reset)
            session.markStopped()
        }
    }

    func testResetAndNextServiceStartInOneChunkReopensTheGate() throws {
        let root = try Self.root(); defer { try? FileManager.default.removeItem(at: root) }
        let session = HvfEngineSession(config: Self.config(root), repoRoot: root) { _ in true }
        try Self.append(Self.boot, root)
        XCTAssertTrue(session.attachToRunningVM())
        try Self.append(Self.resets[0] + Self.boot, root); session.poll()
        XCTAssertTrue(session.sendCtl("hostname"))
        XCTAssertEqual(try Self.control(root), "hostname\n")
        session.markStopped()
    }

    func testAgentReReadyKeepsTheControlGateOpen() throws {
        let root = try Self.root(); defer { try? FileManager.default.removeItem(at: root) }
        let session = HvfEngineSession(config: Self.config(root), repoRoot: root) { _ in true }
        try Self.append(Self.boot, root)
        XCTAssertTrue(session.attachToRunningVM())
        try Self.append("BVAGENT re-READY t t=40\n", root); session.poll()
        XCTAssertTrue(session.sendCtl("hostname"))
        XCTAssertEqual(try Self.control(root), "hostname\n")
        session.markStopped()
    }

    func testWrapperStopDuringRebootDefersShutdownUntilNextServiceStart() throws {
        for reset in Self.resets {
            let root = try Self.root(); defer { try? FileManager.default.removeItem(at: root) }
            let session = HvfEngineSession(config: Self.config(root), repoRoot: root) { _ in true }
            let child = Process(), gate = Pipe()
            child.executableURL = URL(fileURLWithPath: "/bin/sh"); child.arguments = ["-c", "read value"]
            child.standardInput = gate; child.standardOutput = FileHandle.nullDevice
            try child.run()
            defer { try? gate.fileHandleForWriting.close(); child.waitUntilExit() }
            let identity = HvfOwnedRuntimeIdentity(token: UUID(), processID: child.processIdentifier)
            session.process = child; session.ownedProcessIdentity = identity
            try Self.append(Self.boot + reset, root); session.poll()
            guard case .requested = session.stopOwned(expectedToken: identity.token) else {
                return XCTFail("Expected a graceful stop request: \(reset)")
            }
            XCTAssertNil(try? Self.control(root), "shutdown must not enter the reboot gap: \(reset)")
            try Self.append(Self.boot, root); session.poll()
            XCTAssertEqual(try Self.control(root), Self.shutdown, reset)
            session.timer?.invalidate(); session.timer = nil
        }
    }

    func testOwnedStopDuringRebootDefersShutdownUntilNextServiceStart() throws {
        for reset in Self.resets {
            let queue = DispatchQueue(label: "owned-reset-gate-guest")
            let f = try HvfOwnedRuntimeControllerTests.Fixture(guestQueue: queue); defer { f.clean() }
            let session = try Self.ownedSession(f)
            try Self.append(Self.boot, f.base.root); session.poll()
            try Self.append(reset, f.base.root); session.poll()
            f.controller.receive(.message(try f.event("ready", sequence: 1)))
            let ticket = try Self.stop(session, f)
            queue.sync {}
            XCTAssertEqual(try Self.control(f.base.root), "", "shutdown must not enter the reboot gap: \(reset)")
            XCTAssertEqual(ticket.observation.phase, .guestGrace, reset)
            try Self.append(Self.boot, f.base.root); session.poll()
            queue.sync {}
            XCTAssertEqual(try Self.control(f.base.root), Self.shutdown, reset)
        }
    }

    func testOwnedStopAfterAgentReReadyStillRequestsShutdown() throws {
        let queue = DispatchQueue(label: "owned-reready-guest")
        let f = try HvfOwnedRuntimeControllerTests.Fixture(guestQueue: queue); defer { f.clean() }
        let session = try Self.ownedSession(f)
        try Self.append(Self.boot + "BVAGENT re-READY t t=40\n", f.base.root); session.poll()
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        _ = try Self.stop(session, f)
        queue.sync {}
        XCTAssertEqual(try Self.control(f.base.root), Self.shutdown)
    }

    func testResetAfterOwnedShutdownWasSentDoesNotResendIt() throws {
        let queue = DispatchQueue(label: "owned-sent-guest")
        let f = try HvfOwnedRuntimeControllerTests.Fixture(guestQueue: queue); defer { f.clean() }
        let session = try Self.ownedSession(f)
        try Self.append(Self.boot, f.base.root); session.poll()
        f.controller.receive(.message(try f.event("ready", sequence: 1)))
        _ = try Self.stop(session, f)
        queue.sync {}
        try Self.append(Self.resets[1] + Self.boot, f.base.root); session.poll()
        queue.sync {}
        XCTAssertEqual(try Self.control(f.base.root), Self.shutdown)
    }

    private static func ownedSession(_ f: HvfOwnedRuntimeControllerTests.Fixture) throws -> HvfEngineSession {
        try FileManager.default.createDirectory(atPath: f.base.config.evidenceDir, withIntermediateDirectories: true)
        let session = f.base.session()
        session.ownedController = f.controller
        return session
    }

    private static func stop(_ session: HvfEngineSession,
                             _ f: HvfOwnedRuntimeControllerTests.Fixture) throws -> HvfOwnedStopOperation {
        guard case let .accepted(ticket) = session.requestOwnedStop(target: f.controller.identity,
                                                                    operationID: UUID()) else {
            throw CocoaError(.executableRuntimeMismatch)
        }
        return ticket
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

    /// Owned fixtures keep run.log under `evidence/`; plain fixtures use the root.
    private static func log(_ root: URL) -> URL {
        let evidence = root.appendingPathComponent("evidence")
        let dir = FileManager.default.fileExists(atPath: evidence.path) ? evidence : root
        return dir.appendingPathComponent("run.log")
    }

    private static func append(_ text: String, _ root: URL) throws {
        let url = log(root)
        if !FileManager.default.fileExists(atPath: url.path) { FileManager.default.createFile(atPath: url.path, contents: nil) }
        let handle = try FileHandle(forWritingTo: url)
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(text.utf8))
    }

    /// Owned fixtures name the control file `control`; plain fixtures `agent.ctl`.
    private static func control(_ root: URL) throws -> String {
        let owned = root.appendingPathComponent("control")
        let url = FileManager.default.fileExists(atPath: owned.path) ? owned : root.appendingPathComponent("agent.ctl")
        return try String(contentsOf: url, encoding: .utf8)
    }
}
