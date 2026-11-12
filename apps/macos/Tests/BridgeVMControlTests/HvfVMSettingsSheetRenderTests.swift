import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfVMSettingsSheetRenderTests: XCTestCase {
    func testOffscreenSettingsCaptures() async throws {
        guard let output = ProcessInfo.processInfo.environment["BRIDGEVM_SETTINGS_SHEET_OUTPUT"] else {
            throw XCTSkip("Opt-in offscreen settings capture")
        }
        // Reuse the existing capture helper; no window, app launch, or guest is needed.
        NSApplication.shared.setActivationPolicy(.prohibited)
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        var saved = fixture.config()
        saved.virtioDisplayDriverInstalled = true
        saved.networkEnabled = true
        fixture.save(saved)
        let library = fixture.library()
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        // Synthetic owned files make the real disk-size row available. This helper only reports
        // a size; it cannot grow a disk, start a guest, or touch operator media.
        for path in [session.config.targetDiskPath, session.config.uefiVarsPath] {
            try FileManager.default.createDirectory(at: URL(fileURLWithPath: path).deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data().write(to: URL(fileURLWithPath: path))
        }
        let helper = fixture.root.appendingPathComponent("target/release/examples/snapshot_pair_cli")
        try FileManager.default.createDirectory(at: helper.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("#!/bin/sh\n[ \"$1\" = size ] || exit 1\necho 'disk_bytes 68719476736'\n".utf8).write(to: helper)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: helper.path)
        let capture = try HvfAppUICapture(output: URL(fileURLWithPath: output, isDirectory: true))
        for running in [false, true] {
            session.connectionState = running ? .booting : .stopped
            var sheet = HvfVMSettingsSheet(session: session)
            sheet.captureHeight = 2400
            let host = NSHostingView(rootView: sheet.environment(\.colorScheme, .light).background(Color.white))
            host.frame = NSRect(x: 0, y: 0, width: 680, height: 2400)
            host.appearance = NSAppearance(named: .aqua)
            host.layoutSubtreeIfNeeded()
            try await Task.sleep(nanoseconds: 500_000_000)
            try capture.capture(host, name: running ? "settings-running" : "settings-stopped")
        }
        session.connectionState = .stopped
        // Capture the actual advanced content without its attach/readiness onAppear actions.
        let advanced = NSHostingView(rootView: HvfEngineView(session: session).content
            .environment(\.colorScheme, .light).background(Color.white))
        advanced.frame = NSRect(x: 0, y: 0, width: 1000, height: 2600)
        advanced.appearance = NSAppearance(named: .aqua)
        advanced.layoutSubtreeIfNeeded()
        try await Task.sleep(nanoseconds: 500_000_000)
        try capture.capture(advanced, name: "advanced-after-move")
    }
}
