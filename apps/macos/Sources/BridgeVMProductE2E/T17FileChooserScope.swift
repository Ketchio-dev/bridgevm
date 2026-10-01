import ApplicationServices

/// The menu bar never hosts the open panel, its Go To sheet or the Open button, yet an
/// application's AXChildren include it. On the Studio, T17 r63 named the Help menu's search
/// field (`_SC_SEARCH_FIELD`) as the node whose AXChildren kept failing with -25200, which
/// failed every application-wide chooser snapshot (r57–r63). An application root is
/// therefore walked through AXWindows only; every other node keeps both relationships.
enum T17FileChooserScope {
    static func relationships(of node: AXUIElement, root: AXUIElement) -> [String] {
        CFEqual(node, root) && isApplication(root) ? [kAXWindowsAttribute] : T17FileChooserAXTree.relationships
    }
    /// Local identity only: no AX message is sent, so this adds no read that could fail.
    static func isApplication(_ element: AXUIElement) -> Bool {
        var pid: pid_t = 0
        return AXUIElementGetPid(element, &pid) == .success && CFEqual(element, AXUIElementCreateApplication(pid))
    }
}
