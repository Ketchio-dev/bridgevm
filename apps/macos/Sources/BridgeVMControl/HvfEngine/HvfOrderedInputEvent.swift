import Foundation

/// Retained by the queued draft until delivery or cancellation.
final class HvfTextDraftDelivery: Equatable {
    let multiline: Bool
    let onSent: (@MainActor () -> Void)?

    init(multiline: Bool, onSent: (@MainActor () -> Void)?) {
        self.multiline = multiline
        self.onSent = onSent
    }

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
        /// unsent older one.
        var isPointerMove: Bool {
            guard case let .pointer(value) = self else { return false }
            return value.hasPrefix("move:")
        }

        /// What takes this unsent event's place when `newer` is queued right behind it, or nil when both go: a newer
        /// move, or one `scrollunits:` scroll with the same held modifiers at the newer position, each axis summed
        /// within the agent's bound (a trackpad sends scrolls faster than the agent's round trip). A sum of zero
        /// stays two scrolls, since the agent refuses `0,0`. Nothing else merges.
        func absorbing(_ newer: Self) -> Self? {
            if isPointerMove, newer.isPointerMove { return newer }
            guard let old = scrollUnits, let new = newer.scrollUnits,
                  old.at.drop(while: { $0 != ":" }) == new.at.drop(while: { $0 != ":" }) else { return nil }
            let bound = HvfPointerScrollEncoding.maxUnits
            let vertical = max(-bound, min(bound, old.vertical + new.vertical))
            let horizontal = max(-bound, min(bound, old.horizontal + new.horizontal))
            guard vertical != 0 || horizontal != 0 else { return nil }
            return .pointer("scrollunits:\(vertical),\(horizontal)@\(new.at)")
        }

        /// `scrollunits:<v>,<h>@<x>x<y>[:<held modifiers>]` (HvfPointerScrollEncoding): its axes and what follows `@`.
        private var scrollUnits: (vertical: Int, horizontal: Int, at: Substring)? {
            guard case let .pointer(value) = self, value.hasPrefix("scrollunits:") else { return nil }
            let parts = value.dropFirst(12).split(separator: "@", maxSplits: 1)
            let axes = parts.first?.split(separator: ",").compactMap { Int($0) } ?? []
            guard parts.count == 2, axes.count == 2 else { return nil }
            return (axes[0], axes[1], parts[1])
        }

        /// A held key's release (`up:<action>`, HvfModifierChord.up) or a button's (`release`, `releaseall`,
        /// `rightrelease`, `middlerelease`).
        var isRelease: Bool {
            switch self {
            case let .key(value): return value.hasPrefix(HvfModifierChord.up)
            case let .pointer(value):
                let verb = String(value.prefix { $0 != ":" })
                return ["release", "releaseall", "rightrelease", "middlerelease"].contains(verb)
            case .text: return false
            }
        }
    }
}
