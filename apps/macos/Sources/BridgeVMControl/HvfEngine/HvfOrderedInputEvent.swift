import Foundation

/// Retained by the queued draft until delivery or cancellation.
final class HvfTextDraftDelivery: Equatable {
    let onSent: (@MainActor () -> Void)?

    init(onSent: (@MainActor () -> Void)?) { self.onSent = onSent }

    static func == (lhs: HvfTextDraftDelivery, rhs: HvfTextDraftDelivery) -> Bool { lhs === rhs }
}

extension HvfOrderedInputQueue {
    enum Event: Equatable {
        case text(String, draft: HvfTextDraftDelivery? = nil)
        case key(String)
        case pointer(String)
        var byteCount: Int {
            switch self {
            case .text(let value, _), .key(let value), .pointer(let value): return value.utf8.count
            }
        }

        var draft: HvfTextDraftDelivery? {
            if case let .text(_, delivery) = self { return delivery }
            return nil
        }

        /// The guest inserts `move:<x>x<y>` as one absolute MOUSEEVENTF_MOVE without button, wheel
        /// or time data (scripts/win-assets/bvagent-pointer-input.cs), so a newer move supersedes an
        /// unsent older one. Every other event, including a release or scroll at a position, is distinct.
        var isPointerMove: Bool {
            guard case let .pointer(value) = self else { return false }
            return value.hasPrefix("move:")
        }

        /// A held key's release (`up:<action>`, HvfModifierChord.up).
        var isKeyRelease: Bool {
            guard case let .key(value) = self else { return false }
            return value.hasPrefix(HvfModifierChord.up)
        }
    }
}
