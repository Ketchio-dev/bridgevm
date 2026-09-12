#if canImport(AppKit)
import AppKit
import SwiftUI
import QuartzCore
import Darwin

struct HvfFramebufferView: NSViewRepresentable {
    @ObservedObject var session: HvfEngineSession

    func makeNSView(context: Context) -> FBLayerView {
        FBLayerView(session: session)
    }

    func updateNSView(_ nsView: FBLayerView, context: Context) { nsView.updateSession(session) }

    static func dismantleNSView(_ nsView: FBLayerView, coordinator: ()) {
        nsView.teardown()
    }
}

final class FBLayerView: NSView {
    private(set) weak var session: HvfEngineSession?
    private var fileDescriptor: Int32 = -1
    private var mappedPointer: UnsafeMutableRawPointer?
    private var mappedLength = 0
    private var guestSize: CGSize = .zero
    private var lastProcessedSeq: UInt64 = .max
    private var iosurfacePresenter = HvfIOSurfacePresenter()
    private var pointerMoves = HvfPointerMoveMailbox()
    private let pointerCapture = HvfPointerCapture()
    private let inputFocusMonitor = HvfInputFocusMonitor()
    private var frameDisplayLink: CADisplayLink?
    private var pointerTrackingArea: NSTrackingArea?

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    func updateSession(_ next: HvfEngineSession) {
        guard session !== next else { return }
        teardown()
        session = next
        if window != nil { startDisplayLink() }
    }

    init(session: HvfEngineSession) {
        self.session = session
        super.init(frame: .zero)

        wantsLayer = true
        layer?.backgroundColor = NSColor.black.cgColor
        layer?.contentsGravity = .resizeAspect
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        pointerCapture.cancel()
        session?.cancelOrderedInputTarget()
        inputFocusMonitor.watch(window: window) { [weak session = session] in session?.cancelOrderedInputTarget() }
        pointerMoves.reset()

        if window != nil {
            startDisplayLink()
        } else {
            stopDisplayLink()
        }
    }

    override func updateTrackingAreas() {
        if let pointerTrackingArea {
            removeTrackingArea(pointerTrackingArea)
        }

        let trackingArea = NSTrackingArea(
            rect: bounds,
            options: [.mouseMoved, .activeInKeyWindow, .inVisibleRect],
            owner: self,
            userInfo: nil
        )
        pointerTrackingArea = trackingArea
        addTrackingArea(trackingArea)

        super.updateTrackingAreas()
    }

    @objc private func step(_ link: CADisplayLink) {
        defer { flushPendingPointerMove() }
        if presentIOSurfaceIfAvailable(at: link.timestamp) {
            return
        }

        guard ensureMapping(), let mappedPointer else {
            return
        }

        let magic = readUInt32(from: mappedPointer, offset: 0)
        guard magic == 0x42564642 else {
            return
        }

        let sequence0 = readUInt64(from: mappedPointer, offset: 24)
        guard sequence0 & 1 == 0 else {
            return
        }

        // Skip the 8 MB copy + CGImage build when the guest hasn't published a new
        // frame. The device publishes only on RESOURCE_FLUSH, so an idle desktop
        // leaves seq unchanged for long stretches; re-decoding the same frame every
        // CADisplayLink tick just saturates the main thread and stutters interaction.
        guard sequence0 != lastProcessedSeq else {
            return
        }

        let widthValue = readUInt32(from: mappedPointer, offset: 8)
        let heightValue = readUInt32(from: mappedPointer, offset: 12)
        let strideValue = readUInt32(from: mappedPointer, offset: 16)

        guard widthValue > 0, heightValue > 0, strideValue > 0 else {
            resetMapping()
            return
        }

        let width = Int(widthValue)
        let height = Int(heightValue)
        let stride = Int(strideValue)

        guard stride >= width * 4,
              mappedLength >= 64,
              height <= (mappedLength - 64) / stride else {
            resetMapping()
            return
        }

        let pixelByteCount = height * stride

        // Copy the guest frame exactly ONCE into a fresh, CGDataProvider-owned
        // buffer. The old path did two 4 MB copies per frame (mmap -> [UInt8]
        // frameCopy, then Data(frameCopy)); at ~77 fps that was ~300 MB/s of
        // redundant main-thread memcpy. Now: one memcpy, and the provider frees
        // the buffer when the CGImage is released.
        guard let buffer = malloc(pixelByteCount) else {
            return
        }
        Darwin.memcpy(buffer, mappedPointer.advanced(by: 64), pixelByteCount)

        let sequence1 = readUInt64(from: mappedPointer, offset: 24)
        guard sequence1 == sequence0, sequence1 & 1 == 0 else {
            free(buffer)
            return
        }
        lastProcessedSeq = sequence1

        guard let provider = CGDataProvider(
            dataInfo: buffer,
            data: buffer,
            size: pixelByteCount,
            releaseData: { info, _, _ in
                if let info { free(info) }
            }
        ) else {
            free(buffer)
            return
        }

        let bitmapInfo = CGBitmapInfo(
            rawValue: CGBitmapInfo.byteOrder32Little.rawValue
                | CGImageAlphaInfo.noneSkipFirst.rawValue
        )

        guard let image = CGImage(
            width: width,
            height: height,
            bitsPerComponent: 8,
            bitsPerPixel: 32,
            bytesPerRow: stride,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: bitmapInfo,
            provider: provider,
            decode: nil,
            shouldInterpolate: false,
            intent: .defaultIntent
        ) else {
            return
        }

        layer?.contents = image
        guestSize = CGSize(width: CGFloat(width), height: CGFloat(height))
    }

    func teardown() {
        pointerCapture.cancel()
        session?.cancelOrderedInputTarget()
        inputFocusMonitor.stop()
        stopDisplayLink()
        resetMapping()
        iosurfacePresenter.reset()
        pointerMoves.reset()
        guestSize = .zero
        layer?.contents = nil
    }

    override func mouseMoved(with event: NSEvent) {
        pointerMoves.offer(point(event))
    }

    override func mouseDragged(with event: NSEvent) {
        let location = point(event)
        pointerMoves.offer(location)
        if let release = pointerRelease(at: location) { pointerCapture.updateRelease(release) }
    }

    override func rightMouseDragged(with event: NSEvent) { mouseDragged(with: event) }

    override func resignFirstResponder() -> Bool {
        guard super.resignFirstResponder() else { return false }
        pointerCapture.cancel()
        session?.cancelOrderedInputTarget()
        pointerMoves.reset()
        return true
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        pointerMoves.reset()

        guard let session, hasGuestSize else {
            return
        }

        session.sendPointerPress(
            location: point(event),
            viewSize: bounds.size,
            imageSize: guestSize
        )
        if let release = pointerRelease(at: point(event)) { pointerCapture.arm(window: window, release: release) }
    }

    override func mouseUp(with event: NSEvent) {
        pointerMoves.reset()
        if let release = pointerRelease(at: point(event)) {
            pointerCapture.disarm()
            release()
        } else {
            pointerCapture.cancel()
        }
    }

    override func rightMouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        pointerMoves.reset()

        guard let session, hasGuestSize else {
            return
        }

        session.sendPointerRightPress(
            location: point(event),
            viewSize: bounds.size,
            imageSize: guestSize
        )
        if let release = pointerRelease(at: point(event)) { pointerCapture.arm(window: window, release: release) }
    }

    override func rightMouseUp(with event: NSEvent) {
        mouseUp(with: event)
    }

    override func scrollWheel(with event: NSEvent) {
        guard let session, hasGuestSize else {
            return
        }

        guard let delta = HvfScrollDelta.hid(from: event.scrollingDeltaY) else {
            return
        }

        session.sendPointerScroll(
            delta,
            location: point(event),
            viewSize: bounds.size,
            imageSize: guestSize
        )
    }

    override func keyDown(with event: NSEvent) {
        guard let session, hasGuestSize else {
            super.keyDown(with: event)
            return
        }

        if !HvfFramebufferKeyInput.send(event, to: session) { super.keyDown(with: event) }
    }

    private func pointerRelease(at location: CGPoint) -> (() -> Void)? {
        let viewSize = bounds.size, imageSize = guestSize
        guard let session, HvfDisplayCoordinates.absolutePointer(
            location: location, viewSize: viewSize, imageSize: imageSize
        ) != nil else { return nil }
        return { [weak self, weak session] in
            self?.pointerMoves.reset()
            session?.sendPointerRelease(location: location, viewSize: viewSize, imageSize: imageSize)
        }
    }

    private var framebufferPath: String? {
        session.map { $0.config.evidenceDir + "/display.fb" }
    }

    private var iosurfaceSidecarURL: URL? {
        framebufferPath.map { URL(fileURLWithPath: $0 + ".iosurface") }
    }

    private var hasGuestSize: Bool {
        guestSize.width > 0 && guestSize.height > 0
    }

    private func point(_ event: NSEvent) -> CGPoint {
        convert(event.locationInWindow, from: nil)
    }

    private func flushPendingPointerMove() {
        guard let location = pointerMoves.take(), let session, hasGuestSize else { return }
        session.sendPointerMove(location: location, viewSize: bounds.size, imageSize: guestSize)
    }

    private func startDisplayLink() {
        guard frameDisplayLink == nil else {
            return
        }

        if #available(macOS 14.0, *) {
            let link = displayLink(target: self, selector: #selector(step(_:)))
            frameDisplayLink = link
            link.add(to: .main, forMode: .common)
        }
    }

    private func stopDisplayLink() {
        if #available(macOS 14.0, *) {
            frameDisplayLink?.invalidate()
            frameDisplayLink = nil
        }
    }

    private func presentIOSurfaceIfAvailable(at now: TimeInterval) -> Bool {
        guard let iosurfaceSidecarURL,
              let presentation = iosurfacePresenter.presentation(
                from: iosurfaceSidecarURL,
                at: now
              ) else {
            return false
        }
        // Re-assign on every display tick so Core Animation composites pixels
        // updated in place, while descriptor file I/O stays paced separately.
        layer?.contents = presentation.surface
        guestSize = CGSize(width: presentation.width, height: presentation.height)
        if presentation.changed { resetMapping() }
        return true
    }

    private func ensureMapping() -> Bool {
        if fileDescriptor < 0 || mappedPointer == nil {
            return mapFramebuffer()
        }

        guard let currentLength = fileLength(fileDescriptor) else {
            resetMapping()
            return false
        }

        if currentLength != mappedLength {
            resetMapping()
            return mapFramebuffer()
        }

        return true
    }

    private func mapFramebuffer() -> Bool {
        guard let framebufferPath else {
            return false
        }

        let descriptor = HvfFramebufferFileIdentity.open(framebufferPath)
        guard descriptor >= 0 else {
            return false
        }

        guard let length = fileLength(descriptor), length >= 64 else {
            Darwin.close(descriptor)
            return false
        }

        guard let mapping = Darwin.mmap(
            nil,
            length,
            PROT_READ,
            MAP_SHARED,
            descriptor,
            0
        ), mapping != MAP_FAILED else {
            Darwin.close(descriptor)
            return false
        }

        fileDescriptor = descriptor
        mappedPointer = mapping
        mappedLength = length
        return true
    }

    private func resetMapping() {
        if let mappedPointer, mappedLength > 0 {
            Darwin.munmap(mappedPointer, mappedLength)
        }

        mappedPointer = nil
        mappedLength = 0
        lastProcessedSeq = .max

        if fileDescriptor >= 0 {
            Darwin.close(fileDescriptor)
            fileDescriptor = -1
        }
    }

    private func fileLength(_ descriptor: Int32) -> Int? {
        HvfFramebufferFileIdentity.length(descriptor, matching: framebufferPath)
    }

}
#endif
