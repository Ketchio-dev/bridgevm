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

final class FBLayerView: HvfFramebufferTextInput {
    private(set) weak var session: HvfEngineSession?
    var framebufferMapping: HvfFramebufferMapping?
    private var guestSize: CGSize = .zero {
        didSet { if guestSize != oldValue { HvfDisplaySurfaceAccessibility.update(self, guestSize: guestSize) } }
    }
    var lastProcessedSeq: UInt64 = .max
    private let imageFactory: HvfFramebufferImageFactory
    var frameReader = HvfFramebufferFrameReader()
    let frameRequester = HvfFramebufferFrameRequester()
    private var iosurfacePresenter = HvfIOSurfacePresenter()
    private var pointerMoves = HvfPointerMoveMailbox()
    private let pointerCapture = HvfPointerCapture()
    private let inputFocusMonitor = HvfInputFocusMonitor()
    /// The guest's hardware cursor (HvfGuestCursorPresenter.swift).
    let guestCursor = HvfGuestCursorPresenter()
    /// Whether this view's window is the key window, and where the pointer is in the view; tests set them.
    lazy var windowIsKey: () -> Bool = { [weak self] in self?.window?.isKeyWindow == true }
    lazy var hostPointer: () -> CGPoint? = { [weak self] in
        guard let self, let window = self.window else { return nil }
        return self.convert(window.mouseLocationOutsideOfEventStream, from: nil)
    }
    var frameDisplayLink: CADisplayLink?
    private var pointerTrackingArea: NSTrackingArea?

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }
    private var pointerPresses = 0

    func updateSession(_ next: HvfEngineSession) {
        synchronizeTextInputTarget()
        guard session !== next else { return }
        teardown()
        session = next
        inputFocusMonitor.watch(window: window) { [weak self] in self?.windowResignedKey() }
        if window != nil { startDisplayLink() }
        reportDisplaySize(settled: false)
    }

    init(session: HvfEngineSession, imageFactory: HvfFramebufferImageFactory = HvfFramebufferImageFactory()) {
        self.session = session
        self.imageFactory = imageFactory
        super.init(frame: .zero)

        wantsLayer = true
        layer?.backgroundColor = NSColor.black.cgColor
        layer?.contentsGravity = .resizeAspect
        HvfDisplaySurfaceAccessibility.configure(self)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        pointerCapture.cancel()
        cancelGuestInputTarget()
        inputFocusMonitor.watch(window: window) { [weak self] in self?.windowResignedKey() }
        pointerMoves.reset()
        guestCursor.reset()

        if window != nil {
            startDisplayLink()
        } else {
            stopDisplayLink()
        }
        reportDisplaySize(settled: false)
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        reportDisplaySize(settled: false)
    }

    override func viewDidEndLiveResize() {
        super.viewDidEndLiveResize()
        reportDisplaySize(settled: true)
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        reportDisplaySize(settled: false)
    }

    /// The guest display follows the window (HvfDisplayResizeRequest), in points, so a Retina window asks
    /// for its 1x size. SwiftUI lays this view out at its minimum size before the window is shown, so only
    /// a view on screen reports.
    private func reportDisplaySize(settled: Bool) {
        guard let session, window?.isVisible == true else { return }
        session.displayViewResized(to: bounds.size, settled: settled)
    }

    override func updateTrackingAreas() {
        if let pointerTrackingArea {
            removeTrackingArea(pointerTrackingArea)
        }

        let trackingArea = NSTrackingArea(
            rect: bounds,
            options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect],
            owner: self,
            userInfo: nil
        )
        pointerTrackingArea = trackingArea
        addTrackingArea(trackingArea)

        super.updateTrackingAreas()
    }

    /// What the tick did, for the session's `app display pacing:` record (HvfFramebufferDisplayTick.swift).
    @discardableResult func refreshFrame(at timestamp: TimeInterval) -> HvfDisplayTick {
        synchronizeTextInputTarget()
        defer {
            flushPendingPointerMove()
            updateGuestCursor(pointer: hostPointer(), reload: true)
        }
        if presentIOSurfaceIfAvailable(at: timestamp) {
            return .iosurface
        }

        guard ensureMapping(), let mapping = framebufferMapping else {
            invalidateDisplay()
            return .unmapped
        }
        defer { if framebufferMapping === mapping { frameRequester.post(mapping, path: framebufferPath) } }
        let read = frameReader.read(mapping.pointer, length: mapping.length, shown: lastProcessedSeq)
        guard case let .frame(buffer, byteCount, width, height, stride, sequence, published) = read else {
            if case .invalid = read {
                resetMapping(); invalidateDisplay()
            }
            return HvfDisplayTick(unpresented: read)
        }
        guard let image = imageFactory.make(buffer: buffer, byteCount: byteCount,
                                           width: width, height: height, stride: stride), let layer else {
            return .busy
        }

        let previous = lastProcessedSeq
        layer.contents = image
        lastProcessedSeq = sequence
        guestSize = CGSize(width: CGFloat(width), height: CGFloat(height))
        return .presented(sequence: sequence, previous: previous, published: published)
    }

    func teardown() {
        pointerCapture.cancel()
        cancelGuestInputTarget()
        inputFocusMonitor.stop()
        stopDisplayLink()
        resetMapping()
        iosurfacePresenter.reset()
        pointerMoves.reset()
        guestCursor.reset()
        guestSize = .zero
        layer?.contents = nil
    }

    override func mouseMoved(with event: NSEvent) {
        updateGuestCursor(pointer: point(event))
        guard hasGuestSize else { return }
        pointerMoves.offer(point(event))
    }

    override func mouseEntered(with event: NSEvent) {
        updateGuestCursor(pointer: point(event))
    }

    /// Leaving the view: the arrow again, and the guest's sprite at its own position.
    override func mouseExited(with event: NSEvent) {
        updateGuestCursor(pointer: nil)
    }

    override func mouseDragged(with event: NSEvent) {
        updateGuestCursor(pointer: point(event))
        guard hasGuestSize else { return }
        let location = point(event)
        pointerMoves.offer(location)
        if let release = pointerRelease(at: location) { pointerCapture.updateRelease(release) }
    }

    override func rightMouseDragged(with event: NSEvent) { mouseDragged(with: event) }

    override func resignFirstResponder() -> Bool {
        guard super.resignFirstResponder() else { return false }
        pointerCapture.cancel()
        cancelGuestInputTarget()
        pointerMoves.reset()
        return true
    }

    override func mouseDown(with event: NSEvent) {
        cancelModifierTap()
        settleCompositionForClick()
        window?.makeFirstResponder(self)
        pointerMoves.reset()

        guard let session, hasGuestSize else {
            return notePress(session == nil ? "no-session" : "no-guest-size")
        }

        notePress(session.sendPointerPress(
            location: point(event),
            viewSize: bounds.size,
            imageSize: guestSize
        ))
        if let release = pointerRelease(at: point(event)) { pointerCapture.arm(window: window, release: release) }
    }

    /// Accessibility help counts the presses the view received and where the last one went,
    /// so an automated click that never reaches the guest says why (T17 r54/r55).
    private func notePress(_ outcome: String) {
        pointerPresses += 1
        setAccessibilityHelp("presses=\(pointerPresses) last=\(outcome)")
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
        cancelModifierTap()
        settleCompositionForClick()
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
        cancelModifierTap()
        guard let session, hasGuestSize else {
            return
        }

        guard let delta = HvfScrollDelta.hid(from: event.scrollingDeltaY) else {
            return
        }
        pointerMoves.reset()
        session.sendPointerScroll(
            delta,
            location: point(event),
            viewSize: bounds.size,
            imageSize: guestSize
        )
    }

    override var guestInputAvailable: Bool { session != nil && hasGuestSize }
    override var guestInputIdentity: HvfTextInputTargetIdentity? { session?.textInputTarget }
    override func sendGuestKeyEvent(_ event: NSEvent) -> Bool {
        session.map { HvfFramebufferKeyInput.send(event, to: $0) } ?? false
    }
    override func sendGuestText(_ text: String) -> Bool { session?.sendDisplayInput(.text(text)) ?? false }
    override var guestModifierChords: Bool { session?.keyMapping.modifierChords ?? false }
    override func sendGuestModifierTap(_ action: String) {
        if let session { HvfFramebufferKeyInput.sendTap(action, to: session) }
    }
    /// A click commits the syllable being composed when that text reaches the guest before the click.
    /// A clipboard paste would not: the press cancels it (#391), so the syllable is discarded as before.
    private func settleCompositionForClick() {
        _ = synchronizeTextInputTarget()
        commitTextComposition { [weak session] text in session?.displayTextPrecedesPointer(text) == true }
    }

    /// Window resign, first-responder loss, teardown and display loss also drop a pending modifier tap.
    private func cancelGuestInputTarget() {
        cancelModifierTap()
        cancelTextComposition()
        session?.cancelOrderedInputTarget()
    }

    /// The window is no longer key (another window or app): the arrow again.
    func windowResignedKey() {
        cancelGuestInputTarget()
        updateGuestCursor(pointer: nil)
    }

    /// Shows the guest's hardware cursor for the pointer at `pointer` (view points, nil outside the
    /// view); `reload` (each display tick) reads display.cursor again. Only a running VM's file counts,
    /// so a stopped VM gives the arrow back.
    func updateGuestCursor(pointer: CGPoint?, reload: Bool = false) {
        if reload {
            let live = session?.connectionState.guestCursorIsLive == true && hasGuestSize
            guestCursor.refresh(path: live ? guestCursorPath : nil)
        }
        let fit = HvfDisplayFit(viewSize: bounds.size, imageSize: guestSize)
        let presentation = HvfGuestCursorPresentation.decide(
            cursor: guestCursor.state, keyWindow: windowIsKey(), pointer: pointer, fit: fit, imageSize: guestSize)
        guestCursor.apply(presentation, fit: fit, in: layer)
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

    var framebufferPath: String? {
        session.map { $0.config.evidenceDir + "/display.fb" }
    }

    var guestCursorPath: String? {
        session.map { HvfGuestCursorFile.path(evidenceDir: $0.config.evidenceDir) }
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

    private func invalidateDisplay() {
        guard hasGuestSize else { return }
        pointerCapture.cancel()
        cancelGuestInputTarget()
        pointerMoves.reset()
        guestCursor.reset()
        guestSize = .zero
        layer?.contents = nil
    }
}
#endif
