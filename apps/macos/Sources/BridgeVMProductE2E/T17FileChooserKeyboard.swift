import AppKit
import ApplicationServices

/// Focus checks are observations, not an atomic WindowServer focus lock.
enum T17FileChooserKeyboard {
    static func deliver(pid: pid_t, activate: () -> Bool, foreground: () -> pid_t?,
                        postPair: () -> Void) throws {
        guard pid > 1 else { throw T17FileChooser.failure("chooser session key target was invalid") }
        guard activate() else { throw T17FileChooser.failure("chooser session key activation failed") }
        guard foreground() == pid else {
            throw T17FileChooser.failure("chooser session key refused: target was not foreground")
        }
        postPair() // Always finish the matched pair; never replay after focus loss.
        guard foreground() == pid else {
            throw T17FileChooser.failure("chooser session key focus changed after posting")
        }
    }

    static func post(pid: pid_t, code: CGKeyCode, flags: CGEventFlags) throws -> String {
        guard let down = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: true),
              let up = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: false) else {
            throw T17FileChooser.failure("file chooser keyboard event creation failed")
        }
        down.flags = flags; up.flags = flags
        var before = ""
        try deliver(pid: pid, activate: { T17Activation.bringToFront(pid: pid) }, foreground: {
            NSWorkspace.shared.frontmostApplication?.processIdentifier
        }, postPair: {
            before = T17FileChooserDiagnostics.snapshot(pid: pid)
            down.post(tap: .cgSessionEventTap); up.post(tap: .cgSessionEventTap)
        })
        return "route=session,key=\(code)/\(flags.rawValue),before{\(before)},after{\(T17FileChooserDiagnostics.snapshot(pid: pid))}"
    }
}
