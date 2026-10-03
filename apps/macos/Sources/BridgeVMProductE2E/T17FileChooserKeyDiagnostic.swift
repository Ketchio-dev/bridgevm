import AppKit
import ApplicationServices

enum T17FileChooserKeyDiagnostic {
    private enum Phase: String {
        case eventCreation = "event_creation", targetGuard = "target_guard"
        case activation, beforePair = "before_pair", postingPair = "posting_pair"
        case afterPair = "after_pair", complete
    }

    static func post(pid: pid_t, code: CGKeyCode, flags: CGEventFlags,
                     admission: () throws -> Void = {}, activation: (() -> T17ActivationRecord)? = nil) throws -> String {
        let down = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: true)
        let up = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: false)
        var pair: (() -> Void)?
        if let down, let up {
            down.flags = flags; up.flags = flags
            pair = { down.post(tap: .cgSessionEventTap); up.post(tap: .cgSessionEventTap) }
        }
        return try run(pid: pid, code: code, flags: flags,
                       activate: activation ?? { T17Activation.observe(pid: pid) }, foreground: {
            NSWorkspace.shared.frontmostApplication?.processIdentifier
        }, postPair: pair, admission: admission)
    }

    /// Observe the existing guarded delivery, without replay or extra retries.
    static func run(pid: pid_t, code: CGKeyCode, flags: CGEventFlags,
                    activate: () -> T17ActivationRecord, foreground: () -> pid_t?,
                    postPair: (() -> Void)?, admission: () throws -> Void = {}) throws -> String {
        var phase = Phase.eventCreation
        var activation = T17ActivationRecord()
        var front: pid_t?
        var pairPosted = false
        func context() -> String {
            T17FileChooserKeyDiagnosticFormat.detail(phase: phase.rawValue, pid: pid, code: code, flags: flags,
                   activation: activation, front: front, pairPosted: pairPosted)
        }
        guard let postPair else { throw T17FileChooser.failure(context()) }
        phase = .targetGuard
        do {
            try T17FileChooserKeyboard.deliver(pid: pid, activate: {
                phase = .activation
                activation = activate()
                return activation.succeeded
            }, foreground: {
                phase = pairPosted ? .afterPair : .beforePair
                front = foreground()
                return front
            }, admission: admission, postPair: {
                phase = .postingPair
                postPair()
                pairPosted = true
            })
        } catch let blocker as T17Blocker where T17ChooserNativeBudget.isDeadlineFailure(blocker) {
            throw T17FileChooser.failure(String(blocker.detail.prefix(320)) + "; " + context())
        } catch {
            throw T17FileChooser.failure(context())
        }
        phase = .complete
        return context()
    }

}
