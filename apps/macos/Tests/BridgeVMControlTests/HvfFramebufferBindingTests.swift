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
        weak var observed = session
        let view = FBLayerView(session: try XCTUnwrap(session))
        defer { view.teardown() }
        session = nil
        XCTAssertNil(observed)
        XCTAssertNil(view.session)
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
