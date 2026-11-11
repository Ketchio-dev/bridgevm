import AppKit
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfCommandKeyModeTests: XCTestCase {
    /// Mac-like is the default; only the user's saved choice makes Command the Windows key.
    func testCommandKeyIsMacLikeUntilTheUserSavesWindowsKey() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertEqual(session.config.commandKeyMode, .mac, "An absent key is Mac-like")
        XCTAssertEqual(session.keyMapping, HvfHostKeyMapping(commandKey: .mac, modifierChords: true))

        XCTAssertTrue(session.applyCommandKeyMode(.windows) == (.windows, nil))
        let latest = library.latestConfiguration(for: saved)
        XCTAssertEqual(latest.commandKeyMode, "windows")
        XCTAssertEqual(session.config.commandKeyMode, .windows, "the stopped session adopts the saved mode")
        let launch = try XCTUnwrap(HvfEngineConfig.libraryVM(latest, rootURL: library.rootURL,
            e2eHostDiagnosticStopAdmitted: library.e2eHostDiagnosticStopAdmitted))
        XCTAssertEqual(launch.commandKeyMode, .windows)

        // Mac-like again restores the never-touched registration.
        XCTAssertTrue(session.applyCommandKeyMode(.mac) == (.mac, nil))
        XCTAssertNil(library.latestConfiguration(for: saved).commandKeyMode)
        XCTAssertEqual(HvfCommandKeyMode(saved: "hand-edited"), .mac)
    }

    /// The legacy route (the VMM shipped with the app) takes a bare tap and a Command chord; the line
    /// it gets is the Windows key or the Ctrl chord the mode names. A Command chord stays a tap; Option+S is held
    /// from its key-down (HvfKeyHoldTests).
    func testDisplayKeysAndTapsReachTheLegacyRouteByMode() throws {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("command-key-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        for (mode, expected) in [(HvfCommandKeyMode.mac, "KEY ctrl+c\nKEY win\nKEY down:alt+s\n"),
                                 (.windows, "KEY win+c\nKEY win\nKEY down:alt+s\n")] {
            var config = HvfEngineConfig(targetDiskPath: "synthetic-disk", uefiVarsPath: "synthetic-vars",
                evidenceDir: root.appendingPathComponent(mode.rawValue).path, watchdogMs: nil, ramMiB: 6144, smpCpus: 4,
                clipboardSync: true, shareHostDir: nil, shareGuestDir: nil, virtioNet: false, virtioGpu3d: false,
                nvmeBufferedIO: false, ctlFilePath: root.appendingPathComponent("agent.ctl").path)
            config.commandKeyMode = mode
            try FileManager.default.createDirectory(atPath: config.evidenceDir, withIntermediateDirectories: false)
            let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
            for (flags, characters, keyCode) in [(NSEvent.ModifierFlags.command, "c", UInt16(0x08)), (.option, "s", 0x01)] {
                let event = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: flags,
                    timestamp: 0, windowNumber: 0, context: nil, characters: characters,
                    charactersIgnoringModifiers: characters, isARepeat: false, keyCode: keyCode))
                XCTAssertTrue(HvfFramebufferKeyInput.send(event, to: session))
                if flags == .command { HvfFramebufferKeyInput.sendTap("win", to: session) }
            }
            XCTAssertEqual(try String(contentsOf: URL(fileURLWithPath: config.evidenceDir).appendingPathComponent("input.ctl")),
                           expected, mode.rawValue)
        }
    }
}
