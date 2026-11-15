#if canImport(AppKit)
/// A pointer button or motion action; the raw value is the legacy `POINTER` verb. The VMM's HID path has
/// no middle button, so `middlePress` goes only to an agent that takes it.
enum HvfPointerAction: String {
    case click, press, move, release, rightClick = "right-click", rightPress = "right-press", middlePress = "middle-press"
    case rightRelease = "right-release", middleRelease = "middle-release"

    /// The verb the ordered input route uses for this action.
    var orderedVerb: String { rawValue.replacingOccurrences(of: "-", with: "") }

    // The legacy HID route releases its complete button bitmap.
    var legacyVerb: String {
        switch self {
        case .rightRelease, .middleRelease: return "release"
        default: return rawValue
        }
    }

    /// A press can move guest focus without changing the host first responder, so it
    /// invalidates a legacy clipboard paste still aimed at the previous target.
    var mayMoveGuestFocus: Bool {
        switch self {
        case .click, .press, .rightClick, .rightPress, .middlePress: true
        case .move, .release, .rightRelease, .middleRelease: false
        }
    }
}
#endif
