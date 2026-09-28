import AppKit
import ApplicationServices
import Darwin
import Foundation

/// Failure-only metadata. Never reads AX titles, values, descriptions or paths.
enum T17MissingIdentifierDiagnostic {
    struct Label: Hashable {
        let role: String
        let identifier: String
    }
    struct Inventory {
        var nodes = 0
        var limited = false
        var errors = 0
        var labels: [Label] = []
    }
    struct Observation {
        var pidProbe = "unknown"
        var appPresent = false
        var active: Bool?
        var frontmost: Bool?
        var windowsStatus = -1
        var windowsCount: Int?
        var focusStatus = -1
        var focusPresent = false
        var mainStatus = -1
        var mainPresent = false
        var inventory = Inventory()
    }

    private static let knownIDs: Set<String> = [
        "bridgevm.dashboard.advanced", "bridgevm.windows.runtime.view",
        "bridgevm.windows.install.view", "bridgevm.windows.runtime.start",
        "bridgevm.windows.install.stage", "bridgevm.windows.install.failure",
    ]
    private static let knownRoles: Set<String> = [
        "AXApplication", "AXWindow", "AXButton", "AXGroup", "AXScrollArea",
        "AXStaticText", "AXDialog", "AXSheet", "AXUnknown",
    ]

    static func capture(application: AXUIElement, pid: pid_t, identifier: String,
                        timeout: TimeInterval) -> T17Blocker {
        var sample = Observation()
        let probe = Darwin.kill(pid, 0)
        sample.pidProbe = probe == 0 ? "present" : errno == ESRCH ? "missing" : errno == EPERM ? "denied" : "unknown"
        let running = NSRunningApplication(processIdentifier: pid)
        sample.appPresent = running != nil
        sample.active = running?.isActive
        sample.frontmost = NSWorkspace.shared.frontmostApplication.map { $0.processIdentifier == pid }
        let timeoutStatus = AXUIElementSetMessagingTimeout(application, 0.25)
        if timeoutStatus != .success {
            sample.windowsStatus = Int(timeoutStatus.rawValue)
            sample.focusStatus = Int(timeoutStatus.rawValue)
            sample.mainStatus = Int(timeoutStatus.rawValue)
            sample.inventory.errors = 1
            return failure(identifier: identifier, timeout: timeout, observation: sample)
        }
        let windows = query(application, kAXWindowsAttribute as CFString)
        let windowNodes = windows.1 as? [AXUIElement]
        sample.windowsStatus = Int(windows.0.rawValue)
        sample.windowsCount = windowNodes?.count
        let focus = query(application, kAXFocusedWindowAttribute as CFString)
        sample.focusStatus = Int(focus.0.rawValue)
        sample.focusPresent = focus.1.map { CFGetTypeID($0) == AXUIElementGetTypeID() } ?? false
        let main = query(application, kAXMainWindowAttribute as CFString)
        sample.mainStatus = Int(main.0.rawValue)
        sample.mainPresent = main.1.map { CFGetTypeID($0) == AXUIElementGetTypeID() } ?? false
        let roots = [application] + Array((windowNodes ?? []).prefix(8))
        let deadline = Date().addingTimeInterval(3)
        sample.inventory = inventory(roots: roots, budget: { Date() < deadline },
            metadata: { node in
                (try read(node, kAXRoleAttribute as CFString) as? String,
                 try read(node, kAXIdentifierAttribute as CFString) as? String)
            }, related: { try read($0, kAXChildrenAttribute as CFString) as? [AXUIElement] ?? [] },
            same: { CFEqual($0, $1) })
        if (windowNodes?.count ?? 0) > 8 { sample.inventory.limited = true }
        return failure(identifier: identifier, timeout: timeout, observation: sample)
    }

    static func failure(identifier: String, timeout: TimeInterval,
                        observation: Observation) -> T17Blocker {
        T17Blocker(code: "ui-element-missing",
                   detail: format(identifier: identifier, timeout: timeout, observation: observation))
    }

    static func inventory<Node>(roots: [Node], budget: () -> Bool,
                                metadata: (Node) throws -> (String?, String?),
                                related: (Node) throws -> [Node],
                                same: (Node, Node) -> Bool) -> Inventory {
        let cap = 128
        var pending = Array(roots.prefix(cap)), seen: [Node] = []
        var cursor = 0
        var result = Inventory()
        result.limited = roots.count > cap
        var labels = Set<Label>()
        while cursor < pending.count && budget() {
            let node = pending[cursor]; cursor += 1
            if seen.contains(where: { same($0, node) }) { continue }
            seen.append(node); result.nodes += 1
            do {
                let (role, identifier) = try metadata(node)
                if let identifier, knownIDs.contains(identifier) {
                    labels.insert(Label(role: knownRoles.contains(role ?? "") ? role! : "other",
                                        identifier: identifier))
                }
            } catch { result.errors += 1 }
            do {
                for child in try related(node) {
                    if seen.contains(where: { same($0, child) }) || pending.contains(where: { same($0, child) }) { continue }
                    if pending.count == cap { result.limited = true; break }
                    pending.append(child)
                }
            } catch { result.errors += 1 }
        }
        if cursor < pending.count { result.limited = true }
        result.labels = labels.sorted { $0.identifier == $1.identifier ? $0.role < $1.role : $0.identifier < $1.identifier }
        return result
    }

    static func format(identifier: String, timeout: TimeInterval, observation: Observation) -> String {
        let safeIdentifier = identifier.utf8.count <= 96 && identifier.hasPrefix("bridgevm.")
            && identifier.utf8.allSatisfy { (48...57).contains($0) || (65...90).contains($0)
                || (97...122).contains($0) || [45, 46, 95].contains($0) } ? identifier : "other"
        let time = timeout.isFinite && (0...1800).contains(timeout) ? String(timeout) : "unknown"
        let count = observation.windowsStatus == 0 ? observation.windowsCount.flatMap { $0 >= 0 ? String(min(9_999, $0)) : nil } : nil
        let prefix = "required accessibility identifier was not found: \(safeIdentifier); windows=\(count ?? "unanswered") timeout_s=\(time)"
        let safeProbe = ["present", "missing", "denied", "unknown"].contains(observation.pidProbe) ? observation.pidProbe : "unknown"
        let labels = observation.inventory.labels.filter { knownIDs.contains($0.identifier) }.prefix(8)
            .map { "\($0.identifier):\(knownRoles.contains($0.role) ? $0.role : "other")" }.joined(separator: ",")
        let suffix = "; ax_diag=v1,pid_probe=\(safeProbe),ns_app=\(observation.appPresent),active=\(token(observation.active)),front=\(token(observation.frontmost)),"
            + "ax_windows=\(observation.windowsStatus)/\(count ?? "unknown"),ax_focus=\(observation.focusStatus)/\(observation.focusPresent),"
            + "ax_main=\(observation.mainStatus)/\(observation.mainPresent),nodes=\(min(128, max(0, observation.inventory.nodes))),"
            + "limited=\(observation.inventory.limited),errors=\(min(999, max(0, observation.inventory.errors))),known=[\(labels)]"
        return String((prefix + suffix).prefix(512))
    }

    private static func token(_ value: Bool?) -> String { value.map(String.init) ?? "unknown" }
    private static func query(_ node: AXUIElement, _ name: CFString) -> (AXError, CFTypeRef?) {
        var value: CFTypeRef?
        let status = AXUIElementCopyAttributeValue(node, name, &value)
        return (status, value)
    }
    private static func read(_ node: AXUIElement, _ name: CFString) throws -> CFTypeRef? {
        guard AXUIElementSetMessagingTimeout(node, 0.25) == .success else { throw AXReadFailure.failed }
        let (status, value) = query(node, name)
        guard status == .success || status == .noValue || status == .attributeUnsupported else { throw AXReadFailure.failed }
        return value
    }
    private enum AXReadFailure: Error { case failed }
}
