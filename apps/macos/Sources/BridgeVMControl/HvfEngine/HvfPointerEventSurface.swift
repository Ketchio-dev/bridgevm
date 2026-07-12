#if canImport(AppKit)
import AppKit
import SwiftUI

enum HvfScrollDelta {
    static func hid(from value: CGFloat) -> Int8? {
        guard value.isFinite, value != 0 else { return nil }
        let rounded = Int(value.rounded())
        let nonzero = rounded == 0 ? (value > 0 ? 1 : -1) : rounded
        return Int8(max(-127, min(127, nonzero)))
    }
}

struct HvfPointerEventSurface: NSViewRepresentable {
    var onFocus: () -> Void
    var onMove: (CGPoint) -> Void
    var onLeftDown: (CGPoint) -> Void
    var onLeftUp: (CGPoint) -> Void
    var onRightDown: (CGPoint) -> Void
    var onRightUp: (CGPoint) -> Void
    var onScroll: (Int8, CGPoint) -> Void

    func makeNSView(context: Context) -> PointerView {
        let view = PointerView()
        update(view)
        return view
    }

    func updateNSView(_ nsView: PointerView, context: Context) {
        update(nsView)
    }

    private func update(_ view: PointerView) {
        view.onFocus = onFocus
        view.onMove = onMove
        view.onLeftDown = onLeftDown
        view.onLeftUp = onLeftUp
        view.onRightDown = onRightDown
        view.onRightUp = onRightUp
        view.onScroll = onScroll
    }

    final class PointerView: NSView {
        var onFocus: (() -> Void)?
        var onMove: ((CGPoint) -> Void)?
        var onLeftDown: ((CGPoint) -> Void)?
        var onLeftUp: ((CGPoint) -> Void)?
        var onRightDown: ((CGPoint) -> Void)?
        var onRightUp: ((CGPoint) -> Void)?
        var onScroll: ((Int8, CGPoint) -> Void)?
        private var trackingArea: NSTrackingArea?
        private var lastMoveTimestamp: TimeInterval = -.infinity

        override var isFlipped: Bool { true }
        override var acceptsFirstResponder: Bool { false }

        override func updateTrackingAreas() {
            if let trackingArea {
                removeTrackingArea(trackingArea)
            }
            let replacement = NSTrackingArea(
                rect: bounds,
                options: [.activeInKeyWindow, .inVisibleRect, .mouseMoved, .enabledDuringMouseDrag],
                owner: self,
                userInfo: nil
            )
            addTrackingArea(replacement)
            trackingArea = replacement
            super.updateTrackingAreas()
        }

        override func mouseMoved(with event: NSEvent) {
            emitMove(event)
        }

        override func mouseDragged(with event: NSEvent) {
            emitMove(event)
        }

        override func rightMouseDragged(with event: NSEvent) {
            emitMove(event)
        }

        override func mouseDown(with event: NSEvent) {
            onFocus?()
            onLeftDown?(location(of: event))
        }

        override func mouseUp(with event: NSEvent) {
            onLeftUp?(location(of: event))
        }

        override func rightMouseDown(with event: NSEvent) {
            onFocus?()
            onRightDown?(location(of: event))
        }

        override func rightMouseUp(with event: NSEvent) {
            onRightUp?(location(of: event))
        }

        override func scrollWheel(with event: NSEvent) {
            onFocus?()
            guard let delta = HvfScrollDelta.hid(from: event.scrollingDeltaY) else { return }
            onScroll?(delta, location(of: event))
        }

        private func emitMove(_ event: NSEvent) {
            guard event.timestamp - lastMoveTimestamp >= 0.03 else { return }
            lastMoveTimestamp = event.timestamp
            onMove?(location(of: event))
        }

        private func location(of event: NSEvent) -> CGPoint {
            convert(event.locationInWindow, from: nil)
        }
    }
}
#endif
