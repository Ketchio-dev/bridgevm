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
        try T17FileChooserKeyDiagnostic.post(pid: pid, code: code, flags: flags)
    }
}
