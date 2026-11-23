import AppKit
import ApplicationServices
import Foundation

enum T17ActivationNative {
    static func capture(pid: pid_t, timeout: TimeInterval, admitted: () -> Bool) -> T17ActivationRecord {
        let app = AXUIElementCreateApplication(pid)
        let running = NSRunningApplication(processIdentifier: pid)
        // .activateIgnoringOtherApps is deprecated (documented as no effect since macOS 14); it stays until a
        // live T17 run shows that dropping it does not change activation.
        return T17ActivationProbe.measure(
            timeout: timeout, clock: { ProcessInfo.processInfo.systemUptime },
            isActive: { running?.isActive }, activate: { running?.activate(options: [.activateIgnoringOtherApps]) },
            setFront: { AXUIElementSetAttributeValue(app, kAXFrontmostAttribute as CFString, kCFBooleanTrue).rawValue },
            readFront: {
                var value: CFTypeRef?
                let status = AXUIElementCopyAttributeValue(app, kAXFrontmostAttribute as CFString, &value)
                return (status.rawValue, value as? Bool)
            }, pause: { RunLoop.current.run(until: Date().addingTimeInterval($0)) },
            foreground: { NSWorkspace.shared.frontmostApplication?.processIdentifier }, admitted: admitted)
    }
}
