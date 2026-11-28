import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

/// Up to a dozen strips stack over the VM display, which VoiceOver sees only as a picture: each strip and
/// its close button say which notice they are, and a strip says its sentence once as it appears there.
/// The window is deferred and never shown.
@MainActor
final class HvfNoticeStripAccessibilityTests: XCTestCase {
    private final class Model: ObservableObject {
        @Published var renders = 0
        var bodies = 0 // not published: counting a render must not cause one
    }

    private struct Host: View {
        @ObservedObject var model: Model
        let overDisplay: Bool

        var body: some View {
            HvfNoticeStrip(identifier: "test.low-space", title: "low disk space notice",
                           announcement: "Only 2 GB is left.", dismiss: {}) {
                let _ = model.bodies += 1
                Text("Only 2 GB is left. Render \(model.renders)")
            }
            .environment(\.hvfAnnouncesNotices, overDisplay)
        }
    }

    private final class Spoken { var lines: [String] = [] }

    /// Records the announcements instead of posting them.
    private func listening() -> (Spoken, () -> Void) {
        let spoken = Spoken(), original = HvfNoticeAnnouncement.post
        HvfNoticeAnnouncement.post = { spoken.lines.append($0) }
        return (spoken, { HvfNoticeAnnouncement.post = original })
    }

    func testTheCloseButtonNamesItsNotice() {
        XCTAssertEqual(HvfNoticeStrip<EmptyView>.dismissLabel("low disk space notice"), "Dismiss the low disk space notice")
    }

    func testOverTheDisplayAStripSaysItsSentenceOnceAsItAppears() async {
        let (spoken, restore) = listening()
        defer { restore() }
        let model = Model()
        let host = await hosted(Host(model: model, overDisplay: true))
        defer { host.window?.contentView = nil }
        XCTAssertEqual(spoken.lines, ["Only 2 GB is left."])
        let rendered = model.bodies
        for _ in 0..<3 { model.renders += 1; await settle(host) }
        XCTAssertGreaterThan(model.bodies, rendered, "the strip rendered again")
        XCTAssertEqual(spoken.lines, ["Only 2 GB is left."], "a re-render is not a new notice")
    }

    /// The dashboard and Settings lists show some of the same strips, where VoiceOver reads them as usual.
    func testAStripOutsideTheDisplayStaysQuiet() async {
        let (spoken, restore) = listening()
        defer { restore() }
        let host = await hosted(Host(model: Model(), overDisplay: false))
        defer { host.window?.contentView = nil }
        XCTAssertGreaterThan(host.fittingSize.height, 0, "the strip is there")
        XCTAssertEqual(spoken.lines, [])
    }

    private func hosted(_ view: Host) async -> NSHostingView<Host> {
        let host = NSHostingView(rootView: view)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 640, height: 120),
                              styleMask: [], backing: .buffered, defer: true)
        window.contentView = host
        await settle(host)
        return host
    }

    private func settle(_ host: NSView) async {
        for _ in 0..<5 { await Task.yield() }
        host.layoutSubtreeIfNeeded()
    }
}
