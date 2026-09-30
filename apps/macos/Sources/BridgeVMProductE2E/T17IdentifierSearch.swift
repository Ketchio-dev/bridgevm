import ApplicationServices
import Foundation

/// The identifier lookup behind T17Accessibility.element(): three-read application
/// snapshots, projected by role or by the creation probe, polled until the deadline.
/// T17Accessibility supplies only the AX tree walk and attribute reads.
enum T17IdentifierSearch {
    static func find<Node>(
        _ identifier: String, role expectedRole: String?, timeout: TimeInterval,
        now: () -> Date = Date.init,
        pause: () -> Void = { RunLoop.current.run(until: Date().addingTimeInterval(0.1)) },
        snapshotPause: () -> Void = { Thread.sleep(forTimeInterval: 0.2) },
        root: () -> Node, nodes: (Node) throws -> [Node],
        attribute: (Node, String) throws -> String?, missing: () -> Error
    ) throws -> Node {
        try T17IdentifierLookup.poll(identifier, timeout: timeout, now: now, pause: pause, snapshot: {
            try T17ApplicationSnapshot.read(pause: snapshotPause, root: root, nodes: nodes) { found -> Node? in
                guard let expectedRole else {
                    return try T17CreationProbe.find(identifier, in: found, identifier: {
                        try attribute($0, kAXIdentifierAttribute)
                    }, value: { try attribute($0, kAXValueAttribute) })
                }
                return try T17RoleQualifiedIdentity.find(identifier, role: expectedRole, in: found, identifier: {
                    try attribute($0, kAXIdentifierAttribute)
                }, role: { try attribute($0, kAXRoleAttribute) })
            }
        }, missing: missing)
    }
}
