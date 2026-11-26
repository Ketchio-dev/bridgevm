#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfFramebufferBindingTests: XCTestCase {
    func testSessionReplacementClearsOldFrameButSameSessionDoesNot() throws {
        let first = makeSession(), second = makeSession()
        let view = FBLayerView(session: first)
        defer { view.teardown() }
        let layer = try XCTUnwrap(view.layer)
        let provider = try XCTUnwrap(CGDataProvider(data: Data([0, 0, 0, 255]) as CFData))
        let image = try XCTUnwrap(CGImage(
            width: 1, height: 1, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent))
        layer.contents = image
        view.updateSession(first)
        XCTAssertTrue(view.session === first)
        XCTAssertNotNil(layer.contents)
        view.updateSession(second)
        XCTAssertTrue(view.session === second)
        XCTAssertNil(layer.contents)
    }

    func testViewDoesNotKeepSessionAlive() throws {
        var session: HvfEngineSession? = makeSession()
        let observed = WeakReference(session)
        let view = FBLayerView(session: try XCTUnwrap(session))
        defer { view.teardown() }
        session = nil
        XCTAssertNil(observed.object)
        XCTAssertNil(view.session)
    }

    func testStoppedWindowKeepsFinalFrameAndRestartsOnNextBoot() throws {
        try HvfFramebufferAvailabilityFixture.run { _, session, view, path in
            let window = NSWindow(contentRect: view.bounds, styleMask: [], backing: .buffered, defer: true)
            window.contentView = view
            defer { window.contentView = nil }

            try HvfFramebufferAvailabilityFixture.writeFrame(path, sequence: 4)
            session.connectionState = .stopped
            view.updateSession(session)
            view.step(try XCTUnwrap(view.frameDisplayLink))
            XCTAssertEqual(view.lastProcessedSeq, 4)
            XCTAssertNil(view.frameDisplayLink)
            view.updateSession(session)
            XCTAssertNil(view.frameDisplayLink)

            session.connectionState = .booting
            view.updateSession(session)
            try HvfFramebufferAvailabilityFixture.writeFrame(path, sequence: 6)
            view.step(try XCTUnwrap(view.frameDisplayLink))
            XCTAssertEqual(view.lastProcessedSeq, 6)
            XCTAssertNotNil(view.frameDisplayLink)
        }
    }

    func testOcclusionPausesTheLinkAndRevealResumesTheSameDisplay() throws {
        final class Window: NSWindow {
            var showsContent = true
            var shared = false
            override var hasActiveWindowSharingSession: Bool { shared }
            override var occlusionState: NSWindow.OcclusionState { showsContent ? [.visible] : [] }
        }

        try HvfFramebufferAvailabilityFixture.run { fixture, session, view, _ in
            let window = Window(contentRect: view.bounds,
                                styleMask: [], backing: .buffered, defer: true)
            window.contentView = view
            defer { window.contentView = nil }
            let link = try XCTUnwrap(view.frameDisplayLink)
            XCTAssertFalse(link.isPaused)
            session.appPacing.begin()
            defer { _ = session.appPacing.end(evidenceDir: "") }
            let id = ObjectIdentifier(link)
            session.appPacing.display(.presented(sequence: 2, previous: .max, published: 0),
                                      timestamp: 1, target: 1.016, link: id, work: 0)

            let before = try fixture.commands()
            view.mouseMoved(with: try HvfFramebufferAvailabilityFixture.mouse(.mouseMoved))
            window.showsContent = false
            NotificationCenter.default.post(name: NSWindow.didChangeOcclusionStateNotification, object: window)
            XCTAssertTrue(link.isPaused)
            view.mouseMoved(with: try HvfFramebufferAvailabilityFixture.mouse(.mouseMoved))
            window.showsContent = true
            NotificationCenter.default.post(name: NSWindow.didChangeOcclusionStateNotification, object: window)
            XCTAssertFalse(link.isPaused)
            XCTAssertTrue(link === view.frameDisplayLink)
            session.appPacing.display(.presented(sequence: 8, previous: 2, published: 0),
                                      timestamp: 11, target: 11.016, link: id, work: 0)
            XCTAssertEqual(session.appPacing.display.missedVsyncs, 0)
            XCTAssertEqual(session.appPacing.display.firstFrames, 2)
            XCTAssertEqual(session.appPacing.display.skipped, 0)

            view.refreshFrame(at: 11)
            XCTAssertEqual(try fixture.commands(), before, "A stale pointer position must not replay on reveal")
            view.mouseMoved(with: try HvfFramebufferAvailabilityFixture.mouse(.mouseMoved))
            view.refreshFrame(at: 12)
            XCTAssertEqual(try fixture.commands().count, before.count + 1)

            window.shared = true
            window.showsContent = false
            NotificationCenter.default.post(name: NSWindow.didChangeOcclusionStateNotification, object: window)
            XCTAssertFalse(link.isPaused, "A covered window stays live for its active sharing session")
        }
    }

    private func makeSession() -> HvfEngineSession {
        let root = URL(fileURLWithPath: "/tmp/bridgevm-binding-" + UUID().uuidString)
        let config = HvfEngineConfig(
            targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: true, virtioGpu3d: false,
            nvmeBufferedIO: true, ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        return HvfEngineSession(config: config, repoRoot: root) { _ in true }
    }
}
#endif
