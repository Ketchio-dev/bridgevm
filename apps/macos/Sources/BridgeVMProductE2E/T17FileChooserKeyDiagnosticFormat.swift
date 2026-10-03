import ApplicationServices

enum T17FileChooserKeyDiagnosticFormat {
    static func detail(phase: String, pid: pid_t, code: CGKeyCode, flags: CGEventFlags,
                       activation: T17ActivationRecord, front: pid_t?, pairPosted: Bool) -> String {
        func value<T: CustomStringConvertible>(_ item: T?) -> String { item?.description ?? "unknown" }
        let state = phase == "complete" ? "delivered" : "failed"
        let detail = "chooser session key \(state);phase=\(phase);pid=\(pid);key=\(code);flags=\(flags.rawValue)"
            + ";activated=\(activation.succeeded);attempts=\(activation.attempts);elapsed_ms=\(activation.elapsedMilliseconds)"
            + ";native_activation_accepted=\(value(activation.nativeActivationAccepted))"
            + ";ax_set=\(value(activation.axSetCode));ax_read=\(value(activation.axReadCode))"
            + ";native_active=\(value(activation.nativeActive));ax_front=\(value(activation.axFront))"
            + ";activation_front=\(value(activation.observedFrontPID));front=\(value(front));pair_posted=\(pairPosted)"
        return String(detail.prefix(900))
    }
}
