import AppKit
import ApplicationServices

/// Every functional native read uses the admission clock; failure-only diagnostics stay separate.
struct T17FileChooserAXIO {
    let budget: T17ChooserNativeBudget
    func attribute(_ element: AXUIElement, _ name: String) throws -> AnyObject? {
        let names: [String] = try budget.read(.attributeNames) {
            var rawNames: CFArray?
            let result = AXUIElementCopyAttributeNames(element, &rawNames)
            guard result == .success else {
                throw T17FileChooser.failure("file chooser AXAttributeNames read failed; ax_error=\(result.rawValue)")
            }
            guard let names = rawNames as? [String] else { throw T17FileChooser.failure("file chooser AXAttributeNames was invalid") }
            return names
        }
        return try T17SupportedAttribute.read(name, advertised: { names }, value: {
            try budget.read(.attributeValue) { try T17FileChooserAXTree.attribute(element, name) }
        })
    }
    func relationship(_ element: AXUIElement, _ name: String) throws -> AnyObject? {
        try budget.read(.relationship) { try T17FileChooserAXTree.attribute(element, name) }
    }
    func nodes(_ root: AXUIElement) throws -> [AXUIElement] {
        try T17FileChooserAXTree.nodes(root, read: relationship)
    }
    func sheetCandidates(_ panel: AXUIElement) throws -> [AXUIElement] {
        try T17FileChooserNodeLabel.naming(panel, describe: T17FileChooserNodeLabel.chain) {
            try T17FileChooserSheetScope.candidates(of: panel) { try relationship($0, $1) as? [AXUIElement] ?? [] }
        }
    }
    func snapshot<Value>(root: () -> AXUIElement, nodes: (AXUIElement) throws -> [AXUIElement],
                         project: ([AXUIElement]) throws -> Value) throws -> Value {
        try budget.snapshot(root: root, nodes: nodes, project: project)
    }
    func action(_ element: AXUIElement, _ name: String) throws {
        let operation: T17ChooserNativeBudget.Operation = name == kAXPressAction ? .selectionPress : .raise
        let result = try budget.input(operation) { AXUIElementPerformAction(element, name as CFString) }
        guard result == .success else { throw T17FileChooser.failure("file chooser \(name) failed; ax_error=\(result.rawValue)") }
    }
    func setPath(_ path: String, field: AXUIElement) throws -> AXError {
        try budget.input(.pathWrite) { AXUIElementSetAttributeValue(field, kAXValueAttribute as CFString, path as CFString) }
    }
    func activate(pid: pid_t) -> T17ActivationRecord {
        guard let remaining = try? budget.remaining() else { return T17ActivationRecord() }
        return T17Activation.observe(pid: pid, timeout: min(5, remaining), admitted: { budget.activationAdmitted })
    }
    func key(pid: pid_t, code: CGKeyCode, flags: CGEventFlags) throws -> String {
        try T17FileChooserKeyDiagnostic.post(pid: pid, code: code, flags: flags,
            admission: { try budget.check(.keyboard) }, activation: { activate(pid: pid) })
    }
}
