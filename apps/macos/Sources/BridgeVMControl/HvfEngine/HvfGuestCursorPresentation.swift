#if canImport(AppKit)
import CoreGraphics

/// Where the guest's hardware cursor shows, decided without touching AppKit so tests never move the
/// real cursor (HvfGuestCursorPresenter applies it).
struct HvfGuestCursorPresentation: Equatable {
    /// The macOS cursor takes the guest's image: the pointer is on the VM image of the key window.
    /// That is the zero-lag case: macOS draws its own cursor at the pointer on every refresh.
    var macCursor: Bool
    /// The guest's sprite at its own position, in view points, while the host is not driving it.
    var sprite: CGRect?

    /// The macOS arrow and no sprite, exactly as without a hardware cursor.
    static let none = HvfGuestCursorPresentation(macCursor: false, sprite: nil)

    /// `pointer` is the host pointer in view points, nil when it is outside the view.
    static func decide(cursor: HvfGuestCursorState?, keyWindow: Bool, pointer: CGPoint?,
                       fit: HvfDisplayFit?, imageSize: CGSize) -> HvfGuestCursorPresentation {
        // A hidden sprite (Windows drawing the pointer itself) gets the macOS arrow, as today.
        guard let cursor, cursor.visible, cursor.shapeThisBoot, let fit else { return .none }
        if keyWindow, let pointer, fit.contains(pointer) {
            return HvfGuestCursorPresentation(macCursor: true, sprite: nil)
        }
        // The guest position is the sprite's top left (virtio-gpu's cursor position: WDDM's pointer
        // position, which can be negative); the pointer itself is at the hot spot.
        let x = CGFloat(cursor.x), y = CGFloat(cursor.y)
        let pointerX = x + CGFloat(cursor.hotX), pointerY = y + CGFloat(cursor.hotY)
        guard pointerX >= 0, pointerY >= 0, pointerX <= imageSize.width, pointerY <= imageSize.height else {
            return .none
        }
        let origin = CGPoint(x: fit.rect.minX + x * fit.scale, y: fit.rect.minY + y * fit.scale)
        let size = CGSize(width: CGFloat(cursor.width) * fit.scale, height: CGFloat(cursor.height) * fit.scale)
        return HvfGuestCursorPresentation(macCursor: false, sprite: CGRect(origin: origin, size: size))
    }
}

extension HvfConnectionState {
    /// The helper is running, so display.cursor is this boot's: hidden or shown by the guest.
    var guestCursorIsLive: Bool {
        switch self {
        case .booting, .connected: return true
        case .stopped, .stopping, .timedOut: return false
        }
    }
}
#endif
