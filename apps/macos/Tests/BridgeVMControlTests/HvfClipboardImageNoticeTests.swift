import Combine
import Foundation
import XCTest
@testable import BridgeVMControl

/// The too-large image notice follows the probe's exact clipboard lines; it is informational only.
final class HvfClipboardImageNoticeTests: XCTestCase {
    private let macTooLarge = "BVAGENT CLIPSYNC skip image too-large bytes=40000000 width=9000 height=9000 limit-bytes=33554432 limit-side=16384 t=812"
    private let windowsTooLarge = "BVAGENT CLIPSYNC refused guest-image too-large bytes=0 width=20000 height=300 limit-bytes=33554432 limit-side=16384 t=913"

    @MainActor
    private func session() -> HvfEngineSession {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let config = HvfEngineConfig(targetDiskPath: root.appendingPathComponent("disk").path,
            uefiVarsPath: root.appendingPathComponent("vars").path, evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true, shareHostDir: nil, shareGuestDir: nil,
            virtioNet: true, virtioGpu3d: false, nvmeBufferedIO: true,
            ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        return HvfEngineSession(config: config, repoRoot: root) { _ in false }
    }

    @MainActor
    private func feed(_ session: HvfEngineSession, _ lines: [String]) {
        for event in BvAgentEvent.parse(lines: lines) { session.handle(event) }
    }

    @MainActor
    func testTooLargeLinesShowTheirDirectionInKorean() {
        let session = session()
        XCTAssertNil(session.clipboardImageNotice.displayed)
        feed(session, [macTooLarge + "\r"])
        XCTAssertEqual(session.clipboardImageNotice.shown, .macImageTooLarge)
        XCTAssertEqual(session.clipboardImageNotice.displayed?.text, "Mac에서 복사한 이미지가 너무 커서 Windows로 보내지 않았습니다.")
        feed(session, [windowsTooLarge])
        XCTAssertEqual(session.clipboardImageNotice.shown, .windowsImageTooLarge)
        XCTAssertEqual(session.clipboardImageNotice.displayed?.text, "Windows에서 복사한 이미지가 너무 커서 Mac으로 가져오지 않았습니다.")
        XCTAssertEqual(session.clipboardImageNotice.displayed?.help.contains("32MB"), true)
    }

    @MainActor
    func testTheNextSyncOrAGuestResetClearsTheNotice() {
        let session = session()
        feed(session, [macTooLarge])
        feed(session, ["BVAGENT CLIPSYNC host->guest image bytes=4096 width=64 height=32 t=990"])
        XCTAssertNil(session.clipboardImageNotice.shown)
        feed(session, [windowsTooLarge])
        feed(session, ["BVAGENT CLIPSYNC guest->host bytes=5 t=991"])
        XCTAssertNil(session.clipboardImageNotice.shown)
        feed(session, [windowsTooLarge, HvfGuestResetRecord.processRecreation])
        XCTAssertNil(session.clipboardImageNotice.shown)
        feed(session, [macTooLarge])
        session.resetObservedRuntimeState(clearEvents: true)
        XCTAssertNil(session.clipboardImageNotice.shown)
    }

    @MainActor
    func testOtherClipboardLinesAndCommandOutputLeaveTheNoticeAlone() {
        let session = session()
        feed(session, ["BVAGENT CLIPSYNC skip bytes=9000 too-large t=5",
                       "BVAGENT CLIPSYNC refused guest-image unexpected-path bytes=33 width=3 height=2 limit-bytes=33554432 limit-side=16384 t=6",
                       "BVAGENT CLIPSYNC images on t=7",
                       "BVAGENT CMD type notes.txt exit=0", macTooLarge, "BVAGENT END type notes.txt"])
        XCTAssertNil(session.clipboardImageNotice.shown)
    }

    func testImageSyncLinesStillParseAsClipboardSyncs() {
        XCTAssertEqual(BvAgentEvent.parse(lines: ["BVAGENT CLIPSYNC host->guest image bytes=4096 width=64 height=32 t=990"]),
                       [.clipSync(direction: .hostToGuest, bytes: 4096, tMs: 990)])
        XCTAssertEqual(BvAgentEvent.parse(lines: ["BVAGENT CLIPSYNC guest->host image bytes=77 width=1 height=1 t=12"]),
                       [.clipSync(direction: .guestToHost, bytes: 77, tMs: 12)])
        XCTAssertEqual(BvAgentEvent.parse(lines: [macTooLarge]), [.unknown(macTooLarge)])
    }
}
