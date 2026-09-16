import Foundation

enum AppUIDriverAXGraph {
    static func walk<Element>(_ root: Element, limit: Int = 10_000,
                              children: (Element) throws -> [Element],
                              hash: (Element) -> UInt, same: (Element, Element) -> Bool) throws -> [Element] {
        guard limit > 0, limit <= 10_000 else { throw AppUIDriverFailure.protocolLimit }
        var pending = [root], buckets: [UInt: [Element]] = [:], result: [Element] = []
        while let node = pending.popLast() {
            let key = hash(node)
            if buckets[key, default: []].contains(where: { same($0, node) }) { continue }
            guard result.count < limit else { throw AppUIDriverFailure.protocolLimit }
            buckets[key, default: []].append(node)
            result.append(node)
            let next = try children(node)
            guard next.count <= limit, pending.count <= limit - next.count else {
                throw AppUIDriverFailure.protocolLimit
            }
            pending.append(contentsOf: next)
        }
        return result
    }

    static func unique<Element>(_ nodes: [Element], matching: (Element) throws -> Bool) throws -> Element? {
        var result: Element?
        for node in nodes where try matching(node) {
            guard result == nil else { throw AppUIDriverFailure.ambiguousElement }
            result = node
        }
        return result
    }
}
