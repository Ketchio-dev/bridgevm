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

    /// One row per notice transition: the lines in order, then the notice left showing. The
    /// notice stays with the copy it names; only a later copy or a reset clears it.
    @MainActor
    func testTheNoticeStaysWithItsCopyUntilALaterCopy() {
        let windows = "BVAGENT CLIPSYNC refused guest-image too-large bytes=0 width=20000 height=300 limit-bytes=33554432 limit-side=16384 copy=5 t=913"
        let mac = "BVAGENT CLIPSYNC skip image too-large bytes=40000000 width=9000 height=9000 limit-bytes=33554432 limit-side=16384 copy=7 t=812"
        let rows: [(String, [String], HvfClipboardImageNotice.Kind?)] = [
            ("Windows image refused, its text synced",
             [windows, "BVAGENT CLIPSYNC guest->host bytes=4 copy=5 t=914"], .windowsImageTooLarge),
            ("Mac image skipped, its text synced",
             [mac, "BVAGENT CLIPSYNC host->guest bytes=4 copy=7 t=815"], .macImageTooLarge),
            ("a late line about an earlier copy",
             [windows, "BVAGENT CLIPSYNC host->guest bytes=2 copy=4 t=915"], .windowsImageTooLarge),
            ("a later Windows copy",
             [windows, "BVAGENT CLIPSYNC guest->host bytes=4 copy=5 t=914", "BVAGENT CLIPSYNC guest->host bytes=9 copy=6 t=990"], nil),
            ("a later Mac copy",
             [mac, "BVAGENT CLIPSYNC host->guest image bytes=4096 width=64 height=32 copy=8 t=990"], nil),
            ("the other side copies later",
             [windows, "BVAGENT CLIPSYNC host->guest bytes=3 copy=6 t=990"], nil),
            ("a later copy refused for another reason",
             [windows, "BVAGENT CLIPSYNC refused guest-image unexpected-path bytes=33 width=3 height=2 limit-bytes=33554432 limit-side=16384 copy=6 t=990"], nil),
            ("a later copy over the limit too",
             [windows, mac.replacingOccurrences(of: "copy=7", with: "copy=6")], .macImageTooLarge),
            ("startup: the Mac image skipped once images are on",
             ["BVAGENT CLIPSYNC host->guest bytes=4 copy=1 t=50", "BVAGENT CLIPSYNC images on t=60",
              mac.replacingOccurrences(of: "copy=7", with: "copy=2"), "BVAGENT CLIPSYNC host->guest bytes=4 copy=2 t=70"], .macImageTooLarge),
            ("startup: a Windows image refused with its text",
             [windows.replacingOccurrences(of: "copy=5", with: "copy=1"), "BVAGENT CLIPSYNC guest->host bytes=4 copy=1 t=914"], .windowsImageTooLarge),
            ("other clipboard lines",
             [windows, "BVAGENT CLIPSYNC images on t=7", "BVAGENT CLIPSYNC skip bytes=9000 too-large t=8",
              "BVAGENT CLIPSYNC test pasteboard (not the user's clipboard)"], .windowsImageTooLarge),
            ("the same image under a new caption is refused again",
             [windows, "BVAGENT CLIPSYNC guest->host bytes=1 copy=5 t=914",
              windows.replacingOccurrences(of: "copy=5", with: "copy=6"), "BVAGENT CLIPSYNC guest->host bytes=1 copy=6 t=990"],
             .windowsImageTooLarge),
            ("the same image and caption polled again", [windows, "BVAGENT CLIPSYNC guest->host bytes=1 copy=5 t=914"],
             .windowsImageTooLarge),
            ("the image dropped to text only",
             [windows, "BVAGENT CLIPSYNC guest->host bytes=1 copy=5 t=914", "BVAGENT CLIPSYNC guest->host unchanged copy=6 t=990"], nil),
            ("a new image",
             [windows, "BVAGENT CLIPSYNC guest->host bytes=1 copy=5 t=914",
              "BVAGENT CLIPSYNC guest->host image bytes=233 width=8 height=9 copy=6 t=990"], nil),
            ("the Mac drops its image",
             [mac, "BVAGENT CLIPSYNC host->guest bytes=1 copy=7 t=815", "BVAGENT CLIPSYNC host->guest unchanged copy=8 t=990"], nil),
            ("a guest reset", [windows, HvfGuestResetRecord.processRecreation], nil),
            ("a probe without copy numbers", [macTooLarge, "BVAGENT CLIPSYNC guest->host bytes=5 t=991"], nil),
        ]
        var failures: [String] = []
        for (name, lines, expected) in rows {
            let session = session()
            feed(session, lines)
            if session.clipboardImageNotice.shown != expected {
                failures.append("\(name): \(String(describing: session.clipboardImageNotice.shown))")
            }
        }
        let session = session()
        feed(session, [windows])
        session.resetObservedRuntimeState(clearEvents: true)
        if session.clipboardImageNotice.shown != nil { failures.append("session reset") }
        XCTAssertEqual(failures, [])
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

    func testSyncLinesCarryTheirCopyNumber() {
        XCTAssertEqual(BvAgentEvent.parse(lines: ["BVAGENT CLIPSYNC guest->host bytes=4 copy=5 t=914"]),
                       [.clipSync(direction: .guestToHost, bytes: 4, tMs: 914, copy: 5)])
    }

    func testImageSyncLinesStillParseAsClipboardSyncs() {
        XCTAssertEqual(BvAgentEvent.parse(lines: ["BVAGENT CLIPSYNC host->guest image bytes=4096 width=64 height=32 t=990"]),
                       [.clipSync(direction: .hostToGuest, bytes: 4096, tMs: 990)])
        XCTAssertEqual(BvAgentEvent.parse(lines: ["BVAGENT CLIPSYNC guest->host image bytes=77 width=1 height=1 t=12"]),
                       [.clipSync(direction: .guestToHost, bytes: 77, tMs: 12)])
        XCTAssertEqual(BvAgentEvent.parse(lines: [macTooLarge]), [.unknown(macTooLarge)])
    }
}
