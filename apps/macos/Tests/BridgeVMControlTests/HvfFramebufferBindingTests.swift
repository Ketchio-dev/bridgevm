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
