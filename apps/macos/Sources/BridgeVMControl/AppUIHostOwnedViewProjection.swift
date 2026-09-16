#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation

@MainActor
enum AppUIHostOwnedViewProjection {
    enum Stage: String {
        case welcomeTimeout = "welcome-timeout"
        case darkDefault = "dark-default"
        var filename: String { "host-owned-views-\(rawValue).json" }
    }

    static func collect<Node: AnyObject>(
        _ root: Node, stage: Stage, owned: (Node) -> Bool,
        fields: (Node) -> [String: Any], children: (Node) -> [Node],
        accessibility: (Node, inout AppUIHostOwnedAXEntries) -> [String: Any]
    ) -> [String: Any] {
        var budget = AppUIHostOwnedAXEntries()
        var result = AppUIHostAXWalk.collect(root, identity: { value in
            (value as? Node).map { ObjectIdentifier($0) }
        }, inspect: { value in
            guard let node = value as? Node else { return refused([:]) }
            var record: [String: Any] = ["type": String(String(reflecting: type(of: node)).prefix(128))]
            guard owned(node) else { return refused(record) }
            record.merge(fields(node)) { existing, _ in existing }
            guard owned(node) else { return refused(record) }
            if stage == .welcomeTimeout { record["accessibility"] = accessibility(node, &budget) }
            guard owned(node) else { return refused(record) }
            record["same_owned_window"] = true
            return .init(fields: record, children: children(node).map { $0 as Any }, rejected: false)
        })
        let refusals = result.removeValue(forKey: "full_protocol_rejection_count") ?? 0
        result["owned_window_refusal_count"] = refusals
        result["route"] = "exact-owned-nsview-subviews"
        result["ax_queries_enabled"] = stage == .welcomeTimeout
        result["ax_entry_budget"] = budget.summary
        return result
    }

    private static func refused(_ fields: [String: Any]) -> AppUIHostAXWalk.Snapshot {
        var record = fields
        record["same_owned_window"] = false
        return .init(fields: record, children: nil, rejected: true)
    }
}
#endif
