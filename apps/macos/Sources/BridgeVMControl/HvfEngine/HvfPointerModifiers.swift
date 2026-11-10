#if canImport(AppKit)
import AppKit

extension HvfHostKeyMapping {
    /// The guest modifiers held for one pointer action (HvfPointerInputEncoding's `w`, `c`, `a`, `s` field),
    /// mapped as keys are: Control is Ctrl, Option is Alt, Shift is Shift, and Command is Ctrl ('Mac처럼', so
    /// ⌘-click is Ctrl+click) or the Windows key ('Windows 키로'). Right Command is never the Windows key: it is
    /// often the 한/영 key. Caps Lock and Fn are not modifiers here.
    func pointerModifiers(_ flags: NSEvent.ModifierFlags) -> String {
        let command = flags.contains(.command)
        // NX_DEVICELCMDKEYMASK and NX_DEVICERCMDKEYMASK; an event without either (a synthetic one) counts as left.
        let side = flags.rawValue & 0x18
        let win = command && commandKey == .windows && (side & 0x08 != 0 || side == 0)
        let ctrl = flags.contains(.control) || command && commandKey == .mac
        return [win ? "w" : "", ctrl ? "c" : "", flags.contains(.option) ? "a" : "", flags.contains(.shift) ? "s" : ""].joined()
    }
}

extension HvfEngineSession {
    /// Whether the guest takes the middle button now: only an agent that advertised it.
    var guestMiddleButton: Bool { inputDriver.router.middleButton }
}
#endif
