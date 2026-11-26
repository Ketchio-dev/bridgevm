import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryDashboardSettingsSessionTests: XCTestCase {
    func testOpenSettingsFollowTheReplacementSessionUntilBothSheetsClose() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let saved = fixture.config()
        fixture.save(saved)
        let library = fixture.library(probe: HvfClipboardSyncProbe())
        let previous = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let dashboard = LibraryDashboardDetail(config: saved, model: library.model(for: saved), library: library)
        let hosts = (0..<2).map { _ in NSHostingView(rootView: dashboard.settingsSheet) }
        let windows = hosts.map { host in
            let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 680, height: 720),
                                  styleMask: [.titled], backing: .buffered, defer: false)
            window.isReleasedWhenClosed = false
            window.contentView = host
            host.layoutSubtreeIfNeeded()
            return window
        }
        defer { windows.forEach { $0.close() } }
        await waitFor { previous.openSettingsSheets.count == 2 }

        var changed = saved
        changed.memMiB = 8192
        fixture.save(changed)
        library.reload()
        let current = try XCTUnwrap(library.hvfRuntimeDetailSession(for: changed))
        XCTAssertFalse(current === previous)
        hosts.forEach { $0.rootView = dashboard.settingsSheet }
        await waitFor { previous.openSettingsSheets.isEmpty && current.openSettingsSheets.count == 2 }

        windows[0].contentView = nil
        await waitFor { current.openSettingsSheets.count == 1 }
        windows[1].contentView = nil
        await waitFor { current.openSettingsSheets.isEmpty }
        XCTAssertEqual(current.config.ramMiB, 8192)
    }

    private func waitFor(_ condition: () -> Bool, line: UInt = #line) async {
        for _ in 0..<100 {
            if condition() { return }
            try? await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTAssertTrue(condition(), line: line)
    }
}
