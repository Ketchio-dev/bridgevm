#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfFramebufferRetryTests: XCTestCase {
    func testProviderFailureRetriesSamePublishedSequence() throws {
        try assertRetry(failingProvider: true)
    }

    func testImageFailureRetriesSamePublishedSequence() throws {
        try assertRetry(failingProvider: false)
    }

    private func assertRetry(failingProvider: Bool) throws {
        var providerCalls = 0, imageCalls = 0
        let defaults = HvfFramebufferImageFactory()
        var factory = defaults
        factory.provider = { buffer, count in
            providerCalls += 1
            if failingProvider && providerCalls == 1 { return nil }
            return defaults.provider(buffer, count)
        }
        factory.image = { provider, width, height, stride in
            imageCalls += 1
            if !failingProvider && imageCalls == 1 { return nil }
            return defaults.image(provider, width, height, stride)
        }
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let pixels = Data([0x11, 0x22, 0x33, 0xff])
        var frame = Data(count: 64)
        put(UInt32(0x42564642), at: 0, into: &frame)
        put(UInt32(1), at: 8, into: &frame)
        put(UInt32(1), at: 12, into: &frame)
        put(UInt32(4), at: 16, into: &frame)
        put(UInt64(2), at: 24, into: &frame)
        frame.append(pixels)
        let path = root.appendingPathComponent("display.fb")
        try frame.write(to: path)
        let config = HvfEngineConfig(targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false, shareHostDir: nil,
            shareGuestDir: nil, virtioNet: false, virtioGpu3d: false, nvmeBufferedIO: true,
            ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        let view = FBLayerView(session: session, imageFactory: factory)
        defer { view.teardown() }
        let layer = try XCTUnwrap(view.layer)
        view.refreshFrame(at: 0)
        XCTAssertNil(layer.contents)
        XCTAssertEqual(view.accessibilityValue() as? String, "no-frame")
        XCTAssertEqual(providerCalls, 1)
        XCTAssertEqual(imageCalls, failingProvider ? 0 : 1)
        view.refreshFrame(at: 1)
        let contents = try XCTUnwrap(layer.contents)
        XCTAssertEqual(CFGetTypeID(contents as CFTypeRef), CGImage.typeID)
        let image = contents as! CGImage
        XCTAssertEqual(image.width, 1)
        XCTAssertEqual(image.height, 1)
        XCTAssertEqual(try XCTUnwrap(image.dataProvider?.data) as Data, pixels)
        XCTAssertEqual(view.accessibilityValue() as? String, "frame 1x1")
        XCTAssertEqual(providerCalls, 2)
        XCTAssertEqual(imageCalls, failingProvider ? 1 : 2)
        view.refreshFrame(at: 2)
        XCTAssertEqual(providerCalls, 2)
        XCTAssertEqual(imageCalls, failingProvider ? 1 : 2)
        XCTAssertEqual(try Data(contentsOf: path), frame)
        withExtendedLifetime(session) {}
    }

    private func put<Value: FixedWidthInteger>(_ value: Value, at offset: Int, into data: inout Data) {
        var littleEndian = value.littleEndian
        withUnsafeBytes(of: &littleEndian) { data.replaceSubrange(offset..<(offset + $0.count), with: $0) }
    }
}
#endif
