import ApplicationServices

/// The menu bar hosts nothing T17 drives (no menu commands; the open panel, its Go To sheet
/// and the Open button are windows or sheets), yet an application's AXChildren include it. On
/// the Studio, r63 named the Help menu search field (`_SC_SEARCH_FIELD`) as the node failing
/// AXChildren with -25200, failing every app-wide chooser snapshot (r57–r63), then an identifier
/// search (r64). So an application root is walked through AXWindows only; other nodes keep both.
enum T17ApplicationWalkScope {
    static func relationships(of node: AXUIElement, root: AXUIElement) -> [String] {
        CFEqual(node, root) && isApplication(root) ? [kAXWindowsAttribute] : T17AccessibilityTree.relationships
    }
    /// Local identity only: no AX message is sent, so this adds no read that could fail.
    static func isApplication(_ element: AXUIElement) -> Bool {
        var pid: pid_t = 0
        return AXUIElementGetPid(element, &pid) == .success && CFEqual(element, AXUIElementCreateApplication(pid))
    }
}
