#if canImport(AppKit)
/// A pointer button or motion action; the raw value is the legacy `POINTER` verb.
enum HvfPointerAction: String {
    case click, press, move, release, rightClick = "right-click", rightPress = "right-press"

    /// The verb the ordered input route uses for this action.
    var orderedVerb: String { self == .release ? "releaseall" : rawValue.replacingOccurrences(of: "-", with: "") }

    /// A press can move guest focus without changing the host first responder, so it
    /// invalidates a legacy clipboard paste still aimed at the previous target.
    var mayMoveGuestFocus: Bool {
        switch self {
        case .click, .press, .rightClick, .rightPress: true
        case .move, .release: false
        }
    }
}
#endif
