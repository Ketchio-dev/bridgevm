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

    func updateNSView(_ nsView: FBLayerView, context: Context) {}

    static func dismantleNSView(_ nsView: FBLayerView, coordinator: ()) {
        nsView.teardown()
    }
}

final class FBLayerView: NSView {
    private weak var session: HvfEngineSession?
    private var fileDescriptor: Int32 = -1
    private var mappedPointer: UnsafeMutableRawPointer?
    private var mappedLength = 0
    private var frameCopy: [UInt8] = []
    private var guestSize: CGSize = .zero
    private var lastProcessedSeq: UInt64 = .max
    private var frameDisplayLink: CADisplayLink?
    private var pointerTrackingArea: NSTrackingArea?

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }

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
        if frameCopy.count != pixelByteCount {
            frameCopy = [UInt8](repeating: 0, count: pixelByteCount)
        }

        frameCopy.withUnsafeMutableBytes { destination in
            guard let destinationAddress = destination.baseAddress else {
                return
            }
            Darwin.memcpy(
                destinationAddress,
                mappedPointer.advanced(by: 64),
                pixelByteCount
            )
        }

        let sequence1 = readUInt64(from: mappedPointer, offset: 24)
        guard sequence1 == sequence0, sequence1 & 1 == 0 else {
            return
        }
        lastProcessedSeq = sequence1

        let frameData = Data(frameCopy)
        guard let provider = CGDataProvider(data: frameData as CFData) else {
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
        stopDisplayLink()
        resetMapping()
        frameCopy.removeAll(keepingCapacity: false)
        guestSize = .zero
        layer?.contents = nil
    }

    override func mouseMoved(with event: NSEvent) {
        guard let session, hasGuestSize else {
            return
        }

        session.sendPointerMove(
            location: point(event),
            viewSize: bounds.size,
            imageSize: guestSize
        )
    }

    override func mouseDragged(with event: NSEvent) {
        guard let session, hasGuestSize else {
            return
        }

        session.sendPointerMove(
            location: point(event),
            viewSize: bounds.size,
            imageSize: guestSize
        )
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)

        guard let session, hasGuestSize else {
            return
        }

        session.sendPointerPress(
            location: point(event),
            viewSize: bounds.size,
            imageSize: guestSize
        )
    }

    override func mouseUp(with event: NSEvent) {
        guard let session, hasGuestSize else {
            return
        }

        session.sendPointerRelease(
            location: point(event),
            viewSize: bounds.size,
            imageSize: guestSize
        )
    }

    override func rightMouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)

        guard let session, hasGuestSize else {
            return
        }

        session.sendPointerRightPress(
            location: point(event),
            viewSize: bounds.size,
            imageSize: guestSize
        )
    }

    override func rightMouseUp(with event: NSEvent) {
        guard let session, hasGuestSize else {
            return
        }

        session.sendPointerRelease(
            location: point(event),
            viewSize: bounds.size,
            imageSize: guestSize
        )
    }

    override func scrollWheel(with event: NSEvent) {
        guard let session, hasGuestSize else {
            return
        }

        let delta = Int8(clamping: Int(event.scrollingDeltaY.rounded()))
        guard delta != 0 else {
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

        var modifiers: EventModifiers = []
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)

        if flags.contains(.control) {
            modifiers.insert(.control)
        }
        if flags.contains(.option) {
            modifiers.insert(.option)
        }
        if flags.contains(.command) {
            modifiers.insert(.command)
        }
        if flags.contains(.shift) {
            modifiers.insert(.shift)
        }

        switch HvfHostKeyCommand.resolve(
            characters: event.characters ?? "",
            modifiers: modifiers
        ) {
        case let .key(action):
            session.sendKey(action)
        case let .text(text):
            session.sendText(text)
        case .ignored:
            super.keyDown(with: event)
        }
    }

    private var framebufferPath: String? {
        session.map { $0.config.evidenceDir + "/display.fb" }
    }

    private var hasGuestSize: Bool {
        guestSize.width > 0 && guestSize.height > 0
    }

    private func point(_ event: NSEvent) -> CGPoint {
        convert(event.locationInWindow, from: nil)
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

        let descriptor = Darwin.open(framebufferPath, O_RDONLY)
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
        var fileInfo = stat()
        guard Darwin.fstat(descriptor, &fileInfo) == 0,
              fileInfo.st_size >= 0 else {
            return nil
        }

        let length = UInt64(fileInfo.st_size)
        guard length <= UInt64(Int.max) else {
            return nil
        }

        return Int(length)
    }

    private func readUInt32(
        from pointer: UnsafeMutableRawPointer,
        offset: Int
    ) -> UInt32 {
        UInt32(littleEndian: pointer.load(fromByteOffset: offset, as: UInt32.self))
    }

    private func readUInt64(
        from pointer: UnsafeMutableRawPointer,
        offset: Int
    ) -> UInt64 {
        UInt64(littleEndian: pointer.load(fromByteOffset: offset, as: UInt64.self))
    }
}
#endif
