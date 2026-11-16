import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfServiceReadinessTests: XCTestCase {
    private func session() throws -> HvfEngineSession {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("bridgevm-service-ready-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let config = HvfEngineConfig(targetDiskPath: root.appendingPathComponent("disk").path,
            uefiVarsPath: root.appendingPathComponent("vars").path, evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false, shareHostDir: nil,
            shareGuestDir: nil, virtioNet: false, virtioGpu3d: false, nvmeBufferedIO: false,
            ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        try Data().write(to: URL(fileURLWithPath: config.ctlFilePath))
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        session.beginOwnedInputBoot()
        session.connectionState = .booting
        return session
    }

    func testServiceAfterProactivePongConnectsAndNegotiatesInputWithoutReadyHello() throws {
        let session = try session()
        session.handle(.unknown("BVAGENT PONG (proactive) t=8000"))
        session.handle(.serviceStart(tMs: 9000))
        XCTAssertEqual(session.connectionState, .connected(host: "BRIDGEVM"))
        session.poll()
        let commands = try String(contentsOfFile: session.config.ctlFilePath, encoding: .utf8)
        XCTAssertTrue(commands.contains("INPUTCAPS "), "A live resident service must negotiate ordered input")
    }

    func testServicePreservesAdvertisedHostAndDoesNotReopenAStoppingSession() throws {
        let session = try session()
        session.handle(.ready(host: "BRIDGEVM startapps", tMs: 8000))
        session.handle(.serviceStart(tMs: 9000))
        XCTAssertEqual(session.connectionState, .connected(host: "BRIDGEVM startapps"))
        session.connectionState = .stopping
        session.handle(.serviceStart(tMs: 10000))
        XCTAssertEqual(session.connectionState, .stopping)
    }
}
