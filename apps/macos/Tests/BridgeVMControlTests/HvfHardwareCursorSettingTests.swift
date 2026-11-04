#if canImport(AppKit)
import Foundation
import XCTest
@testable import BridgeVMControl

/// The VM's "Windows 커서를 Mac 커서로 표시" setting (HvfHardwareCursorToggle.swift).
@MainActor
final class HvfHardwareCursorSettingTests: XCTestCase {
    private typealias Install = HvfDisplayDriverInstall

    private func session(_ fixture: HvfRuntimeSessionStoreFixture, _ probe: HvfClipboardSyncProbe,
                         driver: Bool = true) throws -> (LibraryModel, VMConfig, HvfEngineSession) {
        var saved = fixture.config()
        saved.virtioDisplayDriverInstalled = driver ? true : nil
        fixture.save(saved)
        let library = fixture.library(probe: probe)
        return (library, saved, try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved)))
    }

    /// Shown with the VirtIO display driver only; saved in vm.json like the other per-VM settings, only
    /// while the VM is stopped; on leaves no key at all.
    func testTheSettingIsSavedWhileStoppedAndOnLeavesNoKey() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        XCTAssertFalse(HvfHardwareCursorToggle.isShown(fixture.config()), "no display driver, no hardware cursor")
        let (library, saved, session) = try session(fixture, probe)
        XCTAssertTrue(HvfHardwareCursorToggle.isShown(session.config.libraryContext?.config))
        XCTAssertTrue(session.guestHardwareCursorWanted, "an absent key is on")

        session.connectionState = .connected(host: "guest")
        XCTAssertEqual(session.applyHardwareCursorToggle(false).message, "VM을 완전히 중지한 뒤 커서 설정을 변경하세요.")
        XCTAssertNil(library.latestConfiguration(for: saved).hardwareCursor)

        session.connectionState = .stopped
        XCTAssertTrue(session.applyHardwareCursorToggle(false) == (false, nil))
        let latest = library.latestConfiguration(for: saved)
        XCTAssertEqual(latest.hardwareCursor, false)
        XCTAssertTrue(library.hvfRuntimeDetailSession(for: latest) === session, "rebound in place")
        XCTAssertNil(library.operationError)
        XCTAssertFalse(session.guestHardwareCursorWanted)

        XCTAssertTrue(session.applyHardwareCursorToggle(true) == (true, nil))
        XCTAssertNil(library.latestConfiguration(for: saved).hardwareCursor)
    }

    /// Off: HWCursor=0 once at the next agent start, even with the device already showing an image, and
    /// never 1. Back on, the automatic HWCursor=1 returns; off again writes 0 again.
    func testOffWritesZeroOnceAtTheNextAgentStartAndNeverOne() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let (library, _, session) = try session(fixture, probe)  // the library saves and rebinds
        XCTAssertTrue(session.config.virtioGpu2d)
        let control = URL(fileURLWithPath: session.config.ctlFilePath)
        let cursor = URL(fileURLWithPath: HvfGuestCursorFile.path(evidenceDir: session.config.evidenceDir))
        for directory in [control, cursor] {
            try FileManager.default.createDirectory(at: directory.deletingLastPathComponent(), withIntermediateDirectories: true)
        }
        func sent() -> [String] {
            ((try? String(contentsOf: control, encoding: .utf8)) ?? "").split(separator: "\n").map(String.init)
        }
        func agentStarts() {
            session.connectionState = .connected(host: "guest")
            session.handle(.serviceStart(tMs: 1))
        }
        func change(_ enabled: Bool) {
            session.connectionState = .stopped
            XCTAssertTrue(session.applyHardwareCursorToggle(enabled) == (enabled, nil))
        }
        let (on, off) = (Install.hardwareCursorCommand, Install.hardwareCursorOffCommand)

        change(false)
        try HvfGuestCursorFixture(shapeThisBoot: true).write(to: cursor)  // HWCursor=1 still in effect
        agentStarts()
        XCTAssertEqual(sent(), [off])
        agentStarts()  // a guest restart in the same app session: already written
        XCTAssertEqual(sent(), [off])

        try FileManager.default.removeItem(at: cursor)  // HWCursor=0 in effect: no image
        change(true)
        agentStarts()
        XCTAssertEqual(sent(), [off, on])
        change(false)
        agentStarts()
        XCTAssertEqual(sent(), [off, on, off])
        XCTAssertNil(library.operationError)
    }
}
#endif
