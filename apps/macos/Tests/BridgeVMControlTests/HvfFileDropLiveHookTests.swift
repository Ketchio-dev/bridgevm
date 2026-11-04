import Foundation
import XCTest
@testable import BridgeVMControl

/// Live-check hook, inert unless BRIDGEVM_DROP_LIVE_CTL is set: drives the drop's own copy path
/// (`copyDroppedFiles`: staging, then the BVDROP control line) against a running headless probe,
/// without the GUI. BRIDGEVM_DROP_LIVE_ITEMS holds the dropped paths, one per line.
@MainActor
final class HvfFileDropLiveHookTests: XCTestCase {
    func testLiveHookStagesItemsForARunningProbe() async throws {
        let env = ProcessInfo.processInfo.environment
        guard let ctl = env["BRIDGEVM_DROP_LIVE_CTL"], !ctl.isEmpty else { return }
        let items = (env["BRIDGEVM_DROP_LIVE_ITEMS"] ?? "").split(separator: "\n").map { URL(fileURLWithPath: String($0)) }
        let root = URL(fileURLWithPath: (ctl as NSString).deletingLastPathComponent)
        let config = HvfEngineConfig(targetDiskPath: root.appendingPathComponent("disk").path,
            uefiVarsPath: root.appendingPathComponent("vars").path, evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false, shareHostDir: nil, shareGuestDir: nil,
            virtioNet: true, virtioGpu3d: false, nvmeBufferedIO: true, ctlFilePath: ctl)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        session.serviceStarted = true // the live driver saw BVAGENT SERVICE start
        session.connectionState = .connected(host: "live")
        await session.copyDroppedFiles(items)?.value
        for event in session.events { print("HOOK \(event.displayText)") }
        print("HOOK notice \(session.fileDrops.notice?.text ?? "none")")
    }
}
