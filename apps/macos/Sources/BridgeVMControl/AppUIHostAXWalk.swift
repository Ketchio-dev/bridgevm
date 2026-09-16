#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation

@MainActor
enum AppUIHostAXWalk {
    static let nodeLimit = 256
    static let depthLimit = 32
    static let childLimit = 128
    struct Snapshot {
        let fields: [String: Any]
        let children: [Any]?
        let rejected: Bool
    }

    static func collect(_ root: Any, identity: (Any) -> ObjectIdentifier?,
                        inspect: (Any) -> Snapshot) -> [String: Any] {
        var pending: [(Any, Int?, Int)] = [(root, nil, 0)]
        var visited = Set<ObjectIdentifier>()
        var nodes: [[String: Any]] = []
        var duplicates = 0, rejected = 0, maximumPending = 1
        var omittedByNode = 0, omittedByDepth = 0, omittedByChild = 0
        var nodeTruncated = false, depthTruncated = false, childTruncated = false
        while let (value, parent, depth) = pending.popLast() {
            if let key = identity(value), !visited.insert(key).inserted {
                duplicates += 1
                continue
            }
            let snapshot = inspect(value)
            let index = nodes.count
            var row = snapshot.fields
            row["index"] = index
            row["parent"] = parent.map { $0 as Any } ?? NSNull()
            row["depth"] = depth
            row["children_read"] = snapshot.children != nil
            row["child_count"] = snapshot.children.map { $0.count as Any } ?? NSNull()
            if snapshot.rejected { rejected += 1 }
            let children = snapshot.children ?? []
            let bounded = min(children.count, childLimit)
            childTruncated = childTruncated || children.count > childLimit
            omittedByChild += children.count - bounded
            let available = max(0, nodeLimit - index - 1 - pending.count)
            let queued = depth < depthLimit ? min(bounded, available) : 0
            if depth >= depthLimit && !children.isEmpty { depthTruncated = true; omittedByDepth += bounded }
            if depth < depthLimit && bounded > available { nodeTruncated = true; omittedByNode += bounded - queued }
            row["children_queued"] = queued
            row["children_omitted"] = children.count - queued
            nodes.append(row)
            pending.append(contentsOf: children.prefix(queued).map { ($0, index, depth + 1) })
            maximumPending = max(maximumPending, pending.count)
        }
        return ["nodes": nodes, "node_count": nodes.count, "node_limit": nodeLimit,
                "depth_limit": depthLimit, "child_limit": childLimit, "maximum_pending": maximumPending,
                "duplicate_count": duplicates, "full_protocol_rejection_count": rejected,
                "children_omitted_by_node_limit": omittedByNode, "children_omitted_by_depth_limit": omittedByDepth,
                "children_omitted_by_child_limit": omittedByChild,
                "node_truncated": nodeTruncated, "depth_truncated": depthTruncated,
                "child_truncated": childTruncated,
                "truncated": nodeTruncated || depthTruncated || childTruncated]
    }
}
#endif
