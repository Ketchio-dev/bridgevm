#if canImport(AppKit)
import CoreGraphics

struct HvfPointerMoveMailbox {
    private var pending: CGPoint?

    mutating func offer(_ point: CGPoint) {
        pending = point
    }

    mutating func take() -> CGPoint? {
        defer { pending = nil }
        return pending
    }

    mutating func reset() {
        pending = nil
    }
}
#endif
