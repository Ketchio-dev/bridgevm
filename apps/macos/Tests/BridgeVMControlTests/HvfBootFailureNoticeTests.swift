import XCTest
@testable import BridgeVMControl

/// A disk with nothing to start (or Windows for x64 PCs) left the window on the firmware's own lines
/// with no sentence. The probe's `BVBOOT no-bootable-disk` line now shows one.
@MainActor
final class HvfBootFailureNoticeTests: XCTestCase {
    func testTheFirmwareFindingNothingToStartIsExplained() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let engine = HvfEngineConfig(targetDiskPath: "disk", uefiVarsPath: "vars", evidenceDir: "unused",
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false, shareHostDir: nil,
            shareGuestDir: nil, virtioNet: false, virtioGpu3d: false, nvmeBufferedIO: false,
            ctlFilePath: "unused/agent.ctl")
        let session = HvfEngineSession(config: engine, repoRoot: root) { _ in false }
        for event in BvAgentEvent.parse(lines: ["BOOT_TIMER milestone name=windows-boot-manager source=serial",
                                               "BVBOOT no-bootable-disk loader=other-cpu\r"]) {
            session.append(event)
        }
        XCTAssertEqual(session.bootFailureNotice.text,
            "Windows can’t start because this disk has Windows for Intel or AMD PCs. Create a VM with Windows 11 for Arm instead.")
        session.resetObservedRuntimeState(clearEvents: true)
        XCTAssertNil(session.bootFailureNotice.text, "a new run starts without it")
        session.append(.unknown("BVBOOT no-bootable-disk"))
        XCTAssertEqual(session.bootFailureNotice.text,
            "Windows can’t start because this VM’s disk has nothing to start. If you imported the disk, check that it has Windows 11 for Arm on it.")
        session.bootFailureNotice.dismiss()
        XCTAssertNil(session.bootFailureNotice.text)
    }
}
