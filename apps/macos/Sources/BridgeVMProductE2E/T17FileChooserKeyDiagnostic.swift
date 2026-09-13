import AppKit
import ApplicationServices

enum T17FileChooserKeyDiagnostic {
    private enum Phase: String {
        case eventCreation = "event_creation", targetGuard = "target_guard"
        case activation, beforePair = "before_pair", postingPair = "posting_pair"
        case afterPair = "after_pair", complete
    }

    static func post(pid: pid_t, code: CGKeyCode, flags: CGEventFlags) throws -> String {
        let down = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: true)
        let up = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: false)
        var pair: (() -> Void)?
        if let down, let up {
            down.flags = flags; up.flags = flags
            pair = { down.post(tap: .cgSessionEventTap); up.post(tap: .cgSessionEventTap) }
        }
        return try run(pid: pid, code: code, flags: flags,
                       activate: { T17Activation.observe(pid: pid) }, foreground: {
            NSWorkspace.shared.frontmostApplication?.processIdentifier
        }, postPair: pair)
    }

    /// Observe the existing guarded delivery, without replay or extra retries.
    static func run(pid: pid_t, code: CGKeyCode, flags: CGEventFlags,
                    activate: () -> T17ActivationRecord, foreground: () -> pid_t?,
                    postPair: (() -> Void)?) throws -> String {
        var phase = Phase.eventCreation
        var activation = T17ActivationRecord()
        var front: pid_t?
        var pairPosted = false
        func context() -> String {
            detail(phase: phase, pid: pid, code: code, flags: flags,
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
            }, postPair: {
                phase = .postingPair
                postPair()
                pairPosted = true
            })
        } catch {
            throw T17FileChooser.failure(context())
        }
        phase = .complete
        return context()
    }

    private static func detail(phase: Phase, pid: pid_t, code: CGKeyCode,
                               flags: CGEventFlags, activation: T17ActivationRecord,
                               front: pid_t?, pairPosted: Bool) -> String {
        func value<T: CustomStringConvertible>(_ item: T?) -> String {
            item?.description ?? "unknown"
        }
        let state = phase == .complete ? "delivered" : "failed"
        let detail = "chooser session key \(state);phase=\(phase.rawValue);pid=\(pid);key=\(code);flags=\(flags.rawValue)"
            + ";activated=\(activation.succeeded);attempts=\(activation.attempts);elapsed_ms=\(activation.elapsedMilliseconds)"
            + ";native_activation_accepted=\(value(activation.nativeActivationAccepted))"
            + ";ax_set=\(value(activation.axSetCode));ax_read=\(value(activation.axReadCode))"
            + ";native_active=\(value(activation.nativeActive));ax_front=\(value(activation.axFront))"
            + ";activation_front=\(value(activation.observedFrontPID));front=\(value(front));pair_posted=\(pairPosted)"
        return String(detail.prefix(900))
    }
}
