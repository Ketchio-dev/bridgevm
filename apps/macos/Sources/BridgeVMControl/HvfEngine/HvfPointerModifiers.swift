#if canImport(AppKit)
import AppKit

extension HvfHostKeyMapping {
    /// The guest modifiers held for one pointer action (HvfPointerInputEncoding's `c`, `a`, `s` field),
    /// mapped as keys are: Control is Ctrl, Option is Alt, Shift is Shift, and in 'Mac처럼' Command is Ctrl
    /// (⌘-click is Ctrl+click). In 'Windows 키로' Command is left out, as before: a Windows key that comes up after
    /// a click opens Start (live check), and Win+click means nothing else. Caps Lock and Fn are not modifiers here.
    func pointerModifiers(_ flags: NSEvent.ModifierFlags) -> String {
        let ctrl = flags.contains(.control) || flags.contains(.command) && commandKey == .mac
        return [ctrl ? "c" : "", flags.contains(.option) ? "a" : "", flags.contains(.shift) ? "s" : ""].joined()
    }
}

extension HvfEngineSession {
    /// Whether the guest takes the middle button now: only an agent that advertised it.
    var guestMiddleButton: Bool { inputDriver.router.middleButton }
}
#endif
