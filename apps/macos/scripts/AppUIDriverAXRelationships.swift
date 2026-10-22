import ApplicationServices
import Foundation

extension AppUIDriverAX {
    func related(_ node: AXUIElement, _ name: String) throws -> [AXUIElement] {
        try prepare(node, attribute: name)
        var available: CFIndex = -1
        let counted = calls.relationships.count(node, name as CFString, &available)
        if counted != .noValue && counted != .attributeUnsupported {
            try status(counted, .getAttributeValueCount, attribute: name)
        }
        try check()
        if counted == .noValue || counted == .attributeUnsupported { return [] }
        guard available >= 0 else { throw AppUIDriverFailure.invalidElement }
        guard available <= 10_000 else { throw AppUIDriverFailure.protocolLimit }
        // An empty array has no valid index zero. A successful count is itself
        // an empty observation, not permission to suppress an AX read failure.
        if available == 0 { return [] }
        // Counting and copying are separate, non-atomic messages. Revalidate
        // ownership/deadline and retain the sentinel so growth cannot hide an
        // oversized array behind the earlier count. Never retry a failed read.
        try prepare(node, attribute: name)
        var value: CFArray?
        let result = calls.relationships.read(node, name as CFString, 0, 10_001, &value)
        if result != .noValue && result != .attributeUnsupported {
            try status(result, .copyAttributeValues, attribute: name)
        }
        try check()
        if result == .noValue || result == .attributeUnsupported { return [] }
        guard let value, CFGetTypeID(value) == CFArrayGetTypeID() else { throw AppUIDriverFailure.invalidElement }
        let count = CFArrayGetCount(value)
        guard count <= 10_000 else { throw AppUIDriverFailure.protocolLimit }
        var nodes: [AXUIElement] = []
        for index in 0..<count {
            guard let pointer = CFArrayGetValueAtIndex(value, index) else { throw AppUIDriverFailure.invalidElement }
            let item = unsafeBitCast(pointer, to: CFTypeRef.self)
            guard CFGetTypeID(item) == AXUIElementGetTypeID() else { throw AppUIDriverFailure.invalidElement }
            let element = unsafeBitCast(item, to: AXUIElement.self)
            try prepare(element, attribute: name, scope: .relatedElement)
            nodes.append(element)
        }
        return nodes
    }
}
