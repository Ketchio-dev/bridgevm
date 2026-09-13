import AppKit
import ApplicationServices
import Foundation

/// Bring the launched product app to the front before driving it, and again
/// before any press that is refused.
///
/// Live on 2026-09-09 the helper found `bridgevm.library.toolbar.create`
/// but its press failed, while a separate plain process reported AXError 0.
/// Foreground contention is a hypothesis; those observations alone do not
/// establish why the helper's press failed. Activation is a bounded recovery
/// attempt, and the caller retains the AX errors if that attempt fails.
enum T17Activation {
    static func bringToFront(pid: pid_t, timeout: TimeInterval = 5) -> Bool {
        observe(pid: pid, timeout: timeout).succeeded
    }

    static func observe(pid: pid_t, timeout: TimeInterval = 5) -> T17ActivationRecord {
        T17ActivationProbe.capture(pid: pid, timeout: timeout)
    }
}
