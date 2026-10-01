import ApplicationServices

/// AppKit attaches a sheet as a direct child of its window, so the Go To sheet's presence and
/// absence are decided among the open panel's direct children only. A deep walk also reached
/// the column view's preview text area, whose AXIdentifier kept failing with -25200 on the
/// Studio (T17 r65, r69); absence then could not be proven after the sheet had closed.
/// Failures reading the panel's own relationships still propagate, so absence stays fail-closed.
enum T17FileChooserSheetScope {
    static func candidates(of panel: AXUIElement, related: (AXUIElement, String) throws -> [AXUIElement]) throws -> [AXUIElement] {
        var result: [AXUIElement] = []
        for name in T17AccessibilityTree.relationships {
            for child in try related(panel, name) where !result.contains(where: { CFEqual($0, child) }) { result.append(child) }
        }
        return result
    }
    static func candidates(of panel: AXUIElement) throws -> [AXUIElement] {
        try T17FileChooserNodeLabel.naming(panel, describe: T17FileChooserNodeLabel.chain) {
            try candidates(of: panel) { try T17FileChooserAXTree.attribute($0, $1) as? [AXUIElement] ?? [] }
        }
    }
}
