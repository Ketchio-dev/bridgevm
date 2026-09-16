#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit

@MainActor
enum AppUIHostOwnedViewFields {
    private static let identifiers: Set<String> = ["bridgevm.first-run.create", "bridgevm.first-run.import"]
    private static let roles: Set<String> = ["AXButton", "AXGroup", "AXSplitGroup", "AXScrollArea", "AXTable",
        "AXRow", "AXColumn", "AXCell", "AXTextField", "AXStaticText", "AXImage", "AXWindow", "AXOutline",
        "AXList", "AXToolbar", "AXSplitter", "AXUnknown"]

    static func common(_ view: NSView) -> [String: Any] {
        ["bounds_local_points": rect(view.bounds), "frame_superview_points": rect(view.frame),
         "hidden": view.isHidden, "hidden_or_hidden_ancestor": view.isHiddenOrHasHiddenAncestor,
         "explicit_appearance": name(view.appearance?.name),
         "effective_appearance": name(view.effectiveAppearance.name),
         "best_match": name(view.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua])),
         "allows_vibrancy": view.allowsVibrancy, "is_outline_view": view is NSOutlineView,
         "is_table_view": view is NSTableView, "is_text_field": view is NSTextField]
    }

    static func accessibility(_ view: NSView, owner: NSWindow, budget: inout AppUIHostOwnedAXEntries) -> [String: Any] {
        let ordinary = budget.project(view.accessibilityChildren() ?? []) { entry($0, owner: owner) }
        let navigation = budget.project(view.accessibilityChildrenInNavigationOrder() ?? []) { entry($0, owner: owner) }
        return ["is_accessibility_element": view.isAccessibilityElement(),
                "ordinary_children": ordinary, "navigation_children": navigation]
    }

    static func entry(_ value: Any, owner: NSWindow? = nil) -> [String: Any] {
        var record: [String: Any] = ["type": String(String(reflecting: type(of: value)).prefix(128)),
            "conforms_full_protocol": value is any NSAccessibilityProtocol,
            "conforms_element_protocol": value is any NSAccessibilityElementProtocol,
            "conforms_button_protocol": value is any NSAccessibilityButton]
        if let view = value as? NSView, owner == nil || view.window !== owner {
            record["view_ownership_refused"] = true
            return record
        }
        guard let object = value as? NSObject else { record["is_nsobject"] = false; return record }
        record["is_nsobject"] = true
        let role = string(object, selector: "accessibilityRole")
        let identifier = string(object, selector: "accessibilityIdentifier")
        record["role_present"] = role != nil
        record["allowed_role"] = role.flatMap { roles.contains($0) ? String($0.prefix(80)) : nil }.map { $0 as Any } ?? NSNull()
        record["identifier_present"] = identifier != nil
        record["allowed_identifier_match"] = identifier.flatMap { identifiers.contains($0) ? $0 : nil }.map { $0 as Any } ?? NSNull()
        return record
    }

    static func rect(_ rect: NSRect) -> [String: Any] {
        AppUIHostGeometryObservation.presentationRect(rect).mapValues { $0.isFinite ? $0 as Any : NSNull() }
    }

    private static func string(_ object: NSObject, selector name: String) -> String? {
        let selector = NSSelectorFromString(name)
        guard object.responds(to: selector) else { return nil }
        return object.perform(selector)?.takeUnretainedValue() as? String
    }

    private static func name(_ value: NSAppearance.Name?) -> Any {
        value.map { String($0.rawValue.prefix(128)) as Any } ?? NSNull()
    }
}
#endif
