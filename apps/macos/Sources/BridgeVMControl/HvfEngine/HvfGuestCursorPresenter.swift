#if canImport(AppKit)
import AppKit
import QuartzCore

/// Shows the guest's hardware cursor (`display.cursor`, HvfGuestCursorFile.swift) in the display view:
/// as the macOS cursor while the pointer is on the VM image of the key window, otherwise as a sprite
/// at the guest's position. It changes the macOS cursor only to the guest's image and back to the
/// arrow, and only while the guest's image is, or was just, the cursor; every way out of the VM image
/// goes back to the arrow. Without cursor commands from the guest nothing here ever shows.
@MainActor
final class HvfGuestCursorPresenter {
    /// Sets the macOS cursor; tests replace it so they never change the real one.
    var setCursor: (NSCursor) -> Void = { $0.set() }
    /// The cursor AppKit shows, to put the guest's back when something else replaced it.
    var currentCursor: () -> NSCursor = { NSCursor.current }
    var uptime: () -> TimeInterval = { ProcessInfo.processInfo.systemUptime }

    private(set) var state: HvfGuestCursorState?
    /// The guest's image as the macOS cursor, while it is.
    private(set) var macCursor: NSCursor?
    let sprite = CALayer()
    /// Holds the sprite, clipped to the VM image as the guest's screen edge clips its pointer.
    let spriteClip: CALayer = {
        let layer = CALayer()
        layer.isHidden = true
        layer.masksToBounds = true
        layer.zPosition = 1
        return layer
    }()
    private var reader = HvfGuestCursorReader()
    private var mapping: HvfFramebufferMapping?
    private var nextOpen: TimeInterval = 0
    private var sequence: UInt64 = .max
    private var image: CGImage?
    private var cursor: (image: CGImage, scale: CGFloat, made: NSCursor)?

    init() {
        spriteClip.addSublayer(sprite)
    }

    /// Takes the newest state of the cursor file at `path` (nil: there is none to show).
    /// Cheap when nothing changed: the file's identity and one sequence load.
    func refresh(path: String?) {
        guard let path else { return drop() }
        if let mapping, mapping.currentLength(matching: path) != mapping.length { drop() }
        if mapping == nil {
            // The file appears with the guest's first cursor command, if ever: look twice a second.
            guard uptime() >= nextOpen else { return }
            nextOpen = uptime() + 0.5
            mapping = HvfFramebufferMapping(path: path)
        }
        guard let mapping else { return }
        switch reader.read(mapping.pointer, length: mapping.length, shown: sequence, serial: state?.serial) {
        case let .state(next, sequence, pixels):
            self.sequence = sequence
            if let pixels { image = Self.image(pixels, width: next.width, height: next.height) }
            state = next
        case .unchanged, .busy:
            break
        case .invalid:
            drop()
        }
    }

    /// Shows `presentation` for the VM image at `fit` in the view.
    func apply(_ presentation: HvfGuestCursorPresentation, fit: HvfDisplayFit?, in layer: CALayer?) {
        if presentation.macCursor, let state, let fit, let made = madeCursor(state, scale: fit.scale) {
            if macCursor !== made || currentCursor() !== made { setCursor(made) }
            macCursor = made
        } else {
            restoreArrow()
        }
        guard let frame = presentation.sprite, let fit, let image, let layer else { return hideSprite() }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        if spriteClip.superlayer !== layer { layer.addSublayer(spriteClip) }
        spriteClip.frame = fit.rect
        sprite.contents = image
        sprite.frame = frame.offsetBy(dx: -fit.rect.minX, dy: -fit.rect.minY)
        spriteClip.isHidden = false
        CATransaction.commit()
    }

    /// The display went away or the view is leaving: the arrow, no sprite, and nothing kept.
    func reset() {
        drop()
        restoreArrow()
        hideSprite()
    }

    private func restoreArrow() {
        guard macCursor != nil else { return }
        macCursor = nil
        setCursor(NSCursor.arrow)
    }

    private func hideSprite() {
        guard !spriteClip.isHidden else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        spriteClip.isHidden = true
        CATransaction.commit()
    }

    private func drop() {
        mapping = nil
        sequence = .max
        state = nil
        image = nil
        cursor = nil
    }

    /// The guest's image at the view's scale, its hot spot from the image's top left.
    private func madeCursor(_ state: HvfGuestCursorState, scale: CGFloat) -> NSCursor? {
        guard let image, scale.isFinite, scale > 0 else { return nil }
        if let cursor, cursor.image === image, cursor.scale == scale { return cursor.made }
        let size = NSSize(width: CGFloat(state.width) * scale, height: CGFloat(state.height) * scale)
        let made = NSCursor(image: NSImage(cgImage: image, size: size),
                            hotSpot: NSPoint(x: CGFloat(state.hotX) * scale, y: CGFloat(state.hotY) * scale))
        cursor = (image, scale, made)
        return made
    }

    /// BGRA with straight alpha, as the device publishes it.
    static func image(_ pixels: [UInt8], width: Int, height: Int) -> CGImage? {
        guard pixels.count == width * height * 4,
              let provider = CGDataProvider(data: Data(pixels) as CFData) else { return nil }
        return CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
                       bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
                       bitmapInfo: CGBitmapInfo(rawValue: CGBitmapInfo.byteOrder32Little.rawValue
                           | CGImageAlphaInfo.first.rawValue),
                       provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
    }
}
#endif
