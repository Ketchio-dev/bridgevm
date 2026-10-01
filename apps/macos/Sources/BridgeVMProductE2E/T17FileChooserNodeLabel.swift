import ApplicationServices
import Foundation

/// Names the AX node whose relationship read failed, so a chooser timeout says where the
/// panel's tree stopped answering (T17 r57–r61). Only roles, subroles and developer
/// identifiers are kept: titles, values and descriptions can carry file or volume names.
enum T17FileChooserNodeLabel {
    static func naming<Node, Result>(_ node: Node, describe: (Node) -> String, _ read: () throws -> Result) throws -> Result {
        // Prefixed: T17FileChooserSnapshot classifies transient reads by the detail's suffix.
        do { return try read() } catch let blocker as T17Blocker {
            throw T17Blocker(code: blocker.code, detail: "node=" + describe(node) + "; " + blocker.detail) }
    }

    /// The failing node first, then up to five ancestors, joined by `<`.
    static func chain(_ node: AXUIElement) -> String {
        var labels: [String] = []
        var current: AXUIElement? = node
        while let element = current, labels.count < 6 {
            labels.append(label(role: text(element, kAXRoleAttribute), subrole: text(element, kAXSubroleAttribute),
                                identifier: text(element, kAXIdentifierAttribute)))
            current = parent(element)
        }
        return labels.joined(separator: "<")
    }

    static func label(role: String?, subrole: String?, identifier: String?) -> String {
        [role ?? "?", subrole ?? "-", identifier ?? "-"].map(sanitized).joined(separator: "/")
    }

    static func sanitized(_ raw: String) -> String {
        let kept = raw.unicodeScalars.filter { $0.isASCII && (CharacterSet.alphanumerics.contains($0) || "-_.?".unicodeScalars.contains($0)) }
        return String(String.UnicodeScalarView(kept.prefix(40)))
    }

    private static func value(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
        var value: CFTypeRef?
        return AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success ? value : nil
    }
    private static func text(_ element: AXUIElement, _ name: String) -> String? { value(element, name) as? String }
    private static func parent(_ element: AXUIElement) -> AXUIElement? {
        guard let raw = value(element, kAXParentAttribute), CFGetTypeID(raw) == AXUIElementGetTypeID() else { return nil }
        return unsafeBitCast(raw, to: AXUIElement.self)  // type ID checked above
    }
}
