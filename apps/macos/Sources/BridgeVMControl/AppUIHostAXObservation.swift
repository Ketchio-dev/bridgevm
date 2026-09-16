#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit

@MainActor
enum AppUIHostAXObservation {
    static let filename = "host-welcome-accessibility.json"
    private static let identifiers: Set<String> = ["bridgevm.first-run.create", "bridgevm.first-run.import"]
    private static let roles: Set<String> = ["AXButton", "AXGroup", "AXSplitGroup", "AXScrollArea", "AXTable",
        "AXRow", "AXColumn", "AXCell", "AXTextField", "AXStaticText", "AXImage", "AXWindow", "AXOutline",
        "AXList", "AXToolbar", "AXSplitter", "AXUnknown"]

    static func save(_ host: AppUIHost, window: NSWindow, content: NSView) throws {
        func admit() throws {
            try Task.checkCancellation()
            try AppUIHost.checkCancellation(output: host.capture.output)
            guard AppUIHost.prepared === host, window.isVisible,
                  window.contentView === content, content.window === window else {
                throw AppUIHostError.refused("Accessibility observation lost visible owned original content")
            }
        }
        try admit()
        let strict = graph(content, publicSelectors: false)
        try admit()
        let selectors = graph(content, publicSelectors: true)
        try admit()
        try write(["schema_version": 1, "kind": "native-app-ui-host-welcome-accessibility",
            "pid": Int(ProcessInfo.processInfo.processIdentifier),
            "observed_uptime": ProcessInfo.processInfo.systemUptime,
            "scope": "exact-owned-content-only-timeout-observation", "text_and_values_recorded": false,
            "record_time_nonatomic": true,
            "ownership": ["prepared_host_matches": AppUIHost.prepared === host,
                          "window_visible": window.isVisible, "current_content_is_original": window.contentView === content,
                          "original_attached_to_window": content.window === window],
            "strict_full_protocol_tree": strict, "public_selector_observation_tree": selectors], capture: host.capture)
    }

    static func write(_ record: [String: Any], capture: AppUIHostCapture) throws {
        try capture.write(record, name: filename, final: true)
    }

    static func graph(_ root: Any, publicSelectors: Bool) -> [String: Any] {
        var result = AppUIHostAXWalk.collect(root, identity: { value in
            (value as? NSObject).map { ObjectIdentifier($0) }
        }, inspect: { value in snapshot(value, publicSelectors: publicSelectors) })
        let nodes = result["nodes"] as? [[String: Any]] ?? []
        result["allowed_identifier_counts"] = Dictionary(uniqueKeysWithValues: identifiers.map { identifier in
            (identifier, nodes.filter { $0["allowed_identifier_match"] as? String == identifier }.count)
        })
        result["route"] = publicSelectors ? "responding-public-getters-observation-only" : "existing-full-protocol-gates"
        return result
    }

    private static func snapshot(_ value: Any, publicSelectors: Bool) -> AppUIHostAXWalk.Snapshot {
        let object = value as? NSObject
        let full = object.flatMap { $0 as? any NSAccessibilityProtocol }
        let childrenSelector = NSSelectorFromString("accessibilityChildren")
        let roleSelector = NSSelectorFromString("accessibilityRole")
        let identifierSelector = NSSelectorFromString("accessibilityIdentifier")
        var fields: [String: Any] = ["type": String(String(reflecting: type(of: value)).prefix(128)),
            "is_nsobject": object != nil, "conforms_full_protocol": full != nil,
            "conforms_element_protocol": value is any NSAccessibilityElementProtocol,
            "conforms_button_protocol": value is any NSAccessibilityButton,
            "responds_children": object?.responds(to: childrenSelector) ?? false,
            "responds_role": object?.responds(to: roleSelector) ?? false,
            "responds_identifier": object?.responds(to: identifierSelector) ?? false,
            "existing_walker_rejection": object == nil ? "not-nsobject" : full == nil ? "not-full-protocol" : "none"]
        var children: [Any]?, role: String?, identifier: String?
        if publicSelectors, let object {
            if object.responds(to: childrenSelector) {
                let value = getter(object, childrenSelector)
                children = value == nil ? [] : value as? [Any]
                fields["children_result_supported"] = value == nil || value is [Any]
            }
            role = getter(object, roleSelector) as? String
            identifier = getter(object, identifierSelector) as? String
        } else if let full {
            children = full.accessibilityChildren() ?? []
            role = full.accessibilityRole()?.rawValue
            identifier = full.accessibilityIdentifier()
        }
        fields["role_present"] = role != nil
        fields["allowed_role"] = role.flatMap { roles.contains($0) ? String($0.prefix(80)) : nil }.map { $0 as Any } ?? NSNull()
        fields["identifier_present"] = identifier != nil
        fields["allowed_identifier_match"] = identifier.flatMap { identifiers.contains($0) ? $0 : nil }.map { $0 as Any } ?? NSNull()
        return AppUIHostAXWalk.Snapshot(fields: fields, children: children, rejected: full == nil)
    }

    private static func getter(_ object: NSObject, _ selector: Selector) -> Any? {
        guard object.responds(to: selector) else { return nil }
        return object.perform(selector)?.takeUnretainedValue()
    }
}
#endif
