import ApplicationServices

/// Product-owned result field for each chooser button. A dismissed panel alone
/// does not prove that the selected path reached the product model.
struct T17ChooserSelectionTarget: Sendable {
    let identifier: String
    let requiredRole: String?

    private static let registered: [String: T17ChooserSelectionTarget] = [
        "bridgevm.create.windows.iso": .init(identifier: "bridgevm.create.windows.iso.selection", requiredRole: nil),
        "bridgevm.create.windows.guest-payload": .init(identifier: "bridgevm.create.windows.guest-payload.selection", requiredRole: nil),
        "bridgevm.create.windows.guest-manifest": .init(identifier: "bridgevm.create.windows.guest-manifest.selection", requiredRole: nil),
        "bridgevm.runtime.share.host.choose": .init(identifier: "bridgevm.runtime.share.host", requiredRole: kAXTextFieldRole as String),
        "bridgevm.first-run.disk.choose": .init(identifier: "bridgevm.first-run.disk.path", requiredRole: kAXTextFieldRole as String),
        "bridgevm.first-run.vars.choose": .init(identifier: "bridgevm.first-run.vars.path", requiredRole: kAXTextFieldRole as String),
        "bridgevm.first-run.vtpm.choose": .init(identifier: "bridgevm.first-run.vtpm.path", requiredRole: kAXTextFieldRole as String),
        "bridgevm.first-run.vtpm-package.choose": .init(identifier: "bridgevm.first-run.vtpm-package.path", requiredRole: kAXTextFieldRole as String),
        "bridgevm.first-run.vtpm-code.choose": .init(identifier: "bridgevm.first-run.vtpm-code.path", requiredRole: kAXTextFieldRole as String),
    ]

    static func resolve(button: String) throws -> Self {
        guard let target = registered[button] else {
            throw T17FileChooser.failure("unregistered chooser result target")
        }
        return target
    }

    func read<Node>(in nodes: [Node], role: (Node) throws -> String?,
                    identifier: (Node) throws -> String?, value: (Node) throws -> String?,
                    same: (Node, Node) -> Bool) throws -> String? {
        let match: Node?
        if let requiredRole {
            match = try T17RoleFirstIdentity.find(
                in: nodes, id: self.identifier, roles: [requiredRole],
                role: role, identifier: identifier, same: same)
        } else {
            // Preserve the established create-screen label lookup.
            match = try nodes.first { try identifier($0) == self.identifier }
        }
        guard let match else { return nil }
        return try value(match)
    }
}
