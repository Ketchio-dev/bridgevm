import AppKit

@MainActor
enum HvfAppUIProbe {
    private static let identifiers: Set<String> = ["bridgevm.first-run.create", "bridgevm.first-run.import",
        "bridgevm.first-run.name", "bridgevm.first-run.disk.path", "bridgevm.first-run.vars.path", "bridgevm.first-run.vtpm.path",
        "bridgevm.first-run.import.commit", "bridgevm.library.overview", "bridgevm.library.search", "bridgevm.library.toolbar.create", "bridgevm.library.overview.create", "bridgevm.library.overview.import",
        "bridgevm.library.import.status", "bridgevm.create.commit"]

    static func save(_ root: NSView, to output: URL, name: String) throws {
        guard ["ui-probe-before-welcome.json", "ui-probe-failure.json"].contains(name)
        else { throw HvfAppUIError.refused("Unknown owned UI probe filename") }
        let record: [String: Any] = ["schema_version": 1, "kind": "owned-native-view-structure",
            "text_and_values_recorded": false, "root_bounds": bounds(root),
            "subviews": graph(root, accessibility: false), "accessibility": graph(root, accessibility: true)]
        let data = try JSONSerialization.data(withJSONObject: record, options: [.prettyPrinted, .sortedKeys])
        try data.write(to: output.appendingPathComponent(name), options: .atomic)
    }

    private static func graph(_ root: NSView, accessibility: Bool) -> [String: Any] {
        var pending: [(NSObject, Int?, Int)] = [(root, nil, 0)]
        var seen = Set<ObjectIdentifier>()
        var nodes: [[String: Any]] = []
        var depthTruncated = false
        while nodes.count < 512, let (object, parent, depth) = pending.popLast() {
            guard seen.insert(ObjectIdentifier(object)).inserted else { continue }
            let children = accessibility ? accessibilityChildren(object) : (object as? NSView)?.subviews ?? []
            var row: [String: Any] = ["index": nodes.count, "parent": parent.map { $0 as Any } ?? NSNull(),
                "class": String(reflecting: type(of: object)), "depth": depth, "child_count": children.count,
                "conforms_accessibility_protocol": object is any NSAccessibilityProtocol,
                "responds_to_children": object.responds(to: NSSelectorFromString("accessibilityChildren")),
                "responds_to_role": object.responds(to: NSSelectorFromString("accessibilityRole")),
                "responds_to_identifier": object.responds(to: NSSelectorFromString("accessibilityIdentifier"))]
            if let view = object as? NSView { row["bounds"] = bounds(view); row["hidden"] = view.isHidden }
            if let role = string(object, selector: "accessibilityRole"), role.hasPrefix("AX"), role.count < 80 {
                row["role"] = role
            }
            if let identifier = string(object, selector: "accessibilityIdentifier") {
                row["has_identifier"] = true
                if identifiers.contains(identifier) { row["static_identifier"] = identifier }
            }
            let index = nodes.count
            nodes.append(row)
            if depth < 32 { pending.append(contentsOf: children.map { ($0, index, depth + 1) }) }
            else if !children.isEmpty { depthTruncated = true }
        }
        return ["nodes": nodes, "node_count": nodes.count, "node_limit": 512, "depth_limit": 32,
            "truncated": !pending.isEmpty || depthTruncated]
    }

    // Some framework elements may answer public AX selectors without declaring
    // the complete protocol. Record that bridge shape without changing actions.
    private static func accessibilityChildren(_ object: NSObject) -> [NSObject] {
        if let element = object as? any NSAccessibilityProtocol {
            return (element.accessibilityChildren() ?? []).compactMap { $0 as? NSObject }
        }
        let selector = NSSelectorFromString("accessibilityChildren")
        guard object.responds(to: selector), let value = object.perform(selector)?.takeUnretainedValue() as? [Any]
        else { return [] }
        return value.compactMap { $0 as? NSObject }
    }

    private static func string(_ object: NSObject, selector name: String) -> String? {
        let selector = NSSelectorFromString(name)
        guard object.responds(to: selector) else { return nil }
        return object.perform(selector)?.takeUnretainedValue() as? String
    }

    private static func bounds(_ view: NSView) -> [String: Double] {
        ["x": Double(view.bounds.minX), "y": Double(view.bounds.minY),
            "width": Double(view.bounds.width), "height": Double(view.bounds.height)]
    }
}
