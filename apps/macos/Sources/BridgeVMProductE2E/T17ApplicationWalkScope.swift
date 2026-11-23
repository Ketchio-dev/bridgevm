import ApplicationServices

/// The menu bar hosts nothing T17 drives (no menu commands; the open panel, its Go To sheet
/// and the Open button are windows or sheets), yet an application's AXChildren include it, and
/// the Help menu search field (`_SC_SEARCH_FIELD`) can fail AXChildren with -25200, failing every
/// app-wide chooser snapshot and identifier search. So an application root is walked through AXWindows only; other nodes keep both.
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
