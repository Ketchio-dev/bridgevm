import ApplicationServices

/// Bounded failure evidence only. Never reads titles, values, URLs or paths.
enum T17FileChooserTreeDiagnostics {
    static func snapshot(_ application: AXUIElement) -> String {
        summarize(roots: [application], metadata: { node in
            (try attribute(node, kAXRoleAttribute) as? String,
             try attribute(node, kAXIdentifierAttribute) as? String)
        }, children: { try attribute($0, kAXChildrenAttribute) as? [AXUIElement] ?? [] })
    }

    private static func attribute(_ node: AXUIElement, _ name: String) throws -> CFTypeRef? {
        var value: CFTypeRef?
        let status = AXUIElementCopyAttributeValue(node, name as CFString, &value)
        guard status == .success || status == .noValue || status == .attributeUnsupported else {
            throw T17FileChooser.failure("AX metadata read failed")
        }
        return value
    }

    static func summarize<Node>(
        roots: [Node], metadata: (Node) throws -> (String?, String?),
        children: (Node) throws -> [Node]
    ) -> String {
        let limit = 128
        var pending = Array(roots.prefix(limit))
        var index = 0, errors = 0, matches = 0
        var limited = roots.count > limit
        var labels: [String] = []
        while index < pending.count {
            let node = pending[index]
            index += 1
            do {
                let (rawRole, rawIdentifier) = try metadata(node)
                let role = T17FileChooserDiagnostics.label(rawRole)
                let identifier = T17FileChooserDiagnostics.label(rawIdentifier)
                if role != "other" && role != "none" || identifier != "other" && identifier != "none" {
                    matches += 1
                    if labels.count < 8 { labels.append(role + "/" + identifier) }
                }
            } catch { errors += 1 }
            do {
                let descendants = try children(node)
                let capacity = limit - pending.count
                limited = limited || descendants.count > capacity
                pending.append(contentsOf: descendants.prefix(capacity))
            } catch { errors += 1 }
        }
        return "nodes=\(index),limited=\(limited),errors=\(errors),matches=\(matches)[\(labels.joined(separator: ","))]"
    }
}
