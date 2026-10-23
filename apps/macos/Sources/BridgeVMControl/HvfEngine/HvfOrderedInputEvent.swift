import Foundation

extension HvfOrderedInputQueue {
    enum Event: Equatable {
        case text(String)
        case key(String)
        case pointer(String)
        var byteCount: Int {
            switch self {
            case .text(let value), .key(let value), .pointer(let value): return value.utf8.count
            }
        }

        /// The guest inserts `move:<x>x<y>` as one absolute MOUSEEVENTF_MOVE without button, wheel
        /// or time data (scripts/win-assets/bvagent-pointer-input.cs), so a newer move supersedes an
        /// unsent older one. Every other event, including a release or scroll at a position, is distinct.
        var isPointerMove: Bool {
            guard case let .pointer(value) = self else { return false }
            return value.hasPrefix("move:")
        }
    }
}
