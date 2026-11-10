import Foundation

/// Turns Mac scroll deltas into Windows wheel units (WHEEL_DELTA 120 = one notch), keeping each
/// axis's fraction for the next event instead of rounding it up. A trackpad or Magic Mouse sends
/// dozens of small point deltas per gesture plus a momentum tail of tiny ones; rounding each up to a
/// whole notch scrolled far too fast.
struct HvfScrollAccumulator {
    /// One AppKit scroll event: `scrollingDeltaX/Y` in points when `precise` (trackpad, Magic Mouse),
    /// in lines otherwise (a wheel mouse). macOS has already applied the user's natural-scrolling choice.
    struct Delta: Equatable {
        var x: Double, y: Double
        var precise: Bool
        /// `phase` is `.began` or `.mayBegin`: fingers touched down for a new gesture.
        var beginsGesture = false
    }

    /// Signed Windows wheel units: vertical > 0 scrolls up (WHEEL), horizontal > 0 scrolls right (HWHEEL).
    struct Units: Equatable { var vertical: Int, horizontal: Int }

    /// Units per guest pixel of precise travel: one notch (120) per 75 guest pixels, between classic
    /// Win32 (a notch is 3 lines, about 50-60 px) and Chromium's 100 px per notch, so content follows
    /// the fingers about one to one.
    static let unitsPerGuestPixel = 1.6
    /// A wheel mouse keeps its feel: one line is one Windows notch.
    static let unitsPerLine = 120.0
    static let notch = 120
    /// Per axis per event (ten notches); the excess of a larger delta is dropped, not carried over.
    static let maxUnitsPerEvent = HvfPointerScrollEncoding.maxUnits

    private var vertical = 0.0, horizontal = 0.0

    /// `guestPixelsPerPoint` is the display's guest pixels per view point (1 / HvfDisplayFit.scale).
    /// `wholeNotches` serves a receiver that takes notches only (the VMM's HID wheel byte, an agent
    /// without SCROLL_UNITS): vertical comes out in multiples of 120 and horizontal is dropped.
    /// Nil when nothing whole has accumulated yet.
    mutating func add(_ delta: Delta, guestPixelsPerPoint: Double, wholeNotches: Bool = false) -> Units? {
        if delta.beginsGesture { vertical = 0; horizontal = 0 }
        let scale = delta.precise ? guestPixelsPerPoint * Self.unitsPerGuestPixel : Self.unitsPerLine
        let up = Self.take(&vertical, adding: delta.y * scale, step: wholeNotches ? Self.notch : 1)
        // AppKit's positive X scrolls toward the left edge; Windows' positive HWHEEL scrolls right.
        let right = wholeNotches ? 0 : Self.take(&horizontal, adding: -delta.x * scale, step: 1)
        if wholeNotches { horizontal = 0 }
        return up == 0 && right == 0 ? nil : Units(vertical: up, horizontal: right)
    }

    /// Adds to one axis (its remainder dropped first when the direction flips) and takes out whole steps.
    private static func take(_ remainder: inout Double, adding units: Double, step: Int) -> Int {
        guard units.isFinite, units != 0 else { return 0 }
        if (remainder < 0) != (units < 0) { remainder = 0 }
        remainder += units
        let whole = (remainder / Double(step)).rounded(.towardZero) * Double(step)
        remainder -= whole
        return Int(max(-Double(maxUnitsPerEvent), min(Double(maxUnitsPerEvent), whole)))
    }
}

#if canImport(AppKit)
import AppKit

extension HvfScrollAccumulator.Delta {
    init(_ event: NSEvent) {
        self.init(x: Double(event.scrollingDeltaX), y: Double(event.scrollingDeltaY),
                  precise: event.hasPreciseScrollingDeltas,
                  beginsGesture: !event.phase.isDisjoint(with: [.began, .mayBegin]))
    }
}
#endif
