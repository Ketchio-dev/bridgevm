import AppKit
import Foundation
import SwiftUI
import XCTest
@testable import BridgeVMControl

/// A Mac keyboard has no PrtSc key; a Mac extended keyboard has F13 in its place, so F13 is Print Screen.
@MainActor
final class HvfPrintScreenKeyTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 8_000)
    private let binding = ["disk", "vars", "evidence", "control"]

    func testF13IsPrintScreenAloneAndInChords() {
        XCTAssertEqual(HvfHostKeyCommand.resolve(characters: "\u{f710}"), .key("printscreen"))
        let mac = HvfHostKeyMapping(commandKey: .mac, modifierChords: true)
        let windows = HvfHostKeyMapping(commandKey: .windows, modifierChords: true)
        for (modifiers, mapping, action) in [(EventModifiers.option, mac, "alt+printscreen"),
                                             (.command, mac, "ctrl+printscreen"), (.command, windows, "win+printscreen")] {
            XCTAssertEqual(HvfHostKeyCommand.resolveChord(keyCode: 0x69, charactersIgnoringModifiers: "\u{f710}",
                                                          modifiers: modifiers, mapping: mapping), .key(action), action)
        }
        XCTAssertEqual(HvfGuestInputEncoding(.key("printscreen"))?.insertedEventCount, 2)
        XCTAssertEqual(HvfGuestInputEncoding(.key("alt+printscreen"))?.insertedEventCount, 4)
    }

    /// The pressed key, through the display's key path, reaches the VMM as `KEY printscreen`.
    func testF13ReachesTheLegacyRouteAsPrintScreen() throws {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("printscreen-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        let config = HvfEngineConfig(targetDiskPath: "synthetic-disk", uefiVarsPath: "synthetic-vars",
            evidenceDir: root.path, watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: false, virtioGpu3d: false, nvmeBufferedIO: false,
            ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        let event = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: .function,
            timestamp: 0, windowNumber: 0, context: nil, characters: "\u{f710}",
            charactersIgnoringModifiers: "\u{f710}", isARepeat: false, keyCode: 0x69))
        XCTAssertTrue(HvfFramebufferKeyInput.send(event, to: session))
        XCTAssertEqual(try String(contentsOf: root.appendingPathComponent("input.ctl"), encoding: .utf8), "KEY printscreen\n")
    }

    /// An agent that cannot build Print Screen would refuse it, so it never gets it; one with the marker does.
    func testPrintScreenGoesToAnAgentOnlyWithItsMarker() {
        for (markers, admitted) in [([String](), false), (["BVINPUT_CHORDS MODIFIER_CHORDS"], false),
                                    (["BVINPUT_CHORDS MODIFIER_CHORDS", "BVINPUT_KEYS PRINTSCREEN"], true)] {
            for key in ["printscreen", "alt+printscreen"] {
                var router = negotiated(markers)
                XCTAssertEqual(router.route(.key(key), binding: binding, now: now), admitted ? .queued : .refused, "\(markers) \(key)")
                XCTAssertFalse(router.failed, key)
            }
        }
    }

    private func negotiated(_ markers: [String]) -> HvfSessionInputRouter {
        var router = HvfSessionInputRouter()
        router.beginOwnedBoot(binding: binding)
        var command = ""
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { command = $0; return true }
        let id = command.split(separator: " ").last.map(String.init) ?? "missing"
        var lines = ["BVAGENT CMD \(command) exit=0", "BVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536"]
        for marker in markers {
            let parts = marker.split(separator: " ")
            lines.append("\(parts[0]) \(id) \(parts[1])")
        }
        lines.append("BVAGENT END \(command)")
        _ = router.poll(binding: binding, serviceReady: true, lines: lines, now: now) { _ in false }
        _ = router.poll(binding: binding, serviceReady: true, lines: [], now: now) { _ in false }
        XCTAssertEqual(router.state, .ready)
        return router
    }
}
