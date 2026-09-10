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
        let app = AXUIElementCreateApplication(pid)
        let running = NSRunningApplication(processIdentifier: pid)
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            if running?.isActive == true { return true }
            running?.activate(options: [.activateIgnoringOtherApps])
            // LaunchServices may not know a Process-launched pid; AX can still raise it.
            AXUIElementSetAttributeValue(app, kAXFrontmostAttribute as CFString, kCFBooleanTrue)
            RunLoop.current.run(until: Date().addingTimeInterval(0.2))
            var value: CFTypeRef?
            if AXUIElementCopyAttributeValue(app, kAXFrontmostAttribute as CFString, &value) == .success,
               (value as? Bool) == true { return true }
        } while Date() < deadline
        return running?.isActive == true
    }
}
