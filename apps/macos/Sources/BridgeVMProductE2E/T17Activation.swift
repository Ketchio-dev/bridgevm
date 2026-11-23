import AppKit
import ApplicationServices
import Foundation

/// Bring the launched product app to the front before driving it, and again
/// before any press that is refused.
///
/// A found element's press can fail while another process's press succeeds;
/// foreground contention is suspected, not proven. Activation is a bounded
/// recovery attempt, and the caller retains the AX errors if that attempt fails.
enum T17Activation {
    static func bringToFront(pid: pid_t, timeout: TimeInterval = 5) -> Bool {
        observe(pid: pid, timeout: timeout).succeeded
    }

    static func observe(pid: pid_t, timeout: TimeInterval = 5, admitted: () -> Bool = { true }) -> T17ActivationRecord {
        T17ActivationProbe.capture(pid: pid, timeout: timeout, admitted: admitted)
    }
}
