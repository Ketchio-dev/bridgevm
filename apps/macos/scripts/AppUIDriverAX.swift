import ApplicationServices
import Foundation

struct AppUIDriverAX: AppUIDriverAXAccess {
    let pid: pid_t
    let deadline: Double
    let cancelled: () throws -> Bool
    var now: () -> Double = { ProcessInfo.processInfo.systemUptime }

    func application() -> AXUIElement { AXUIElementCreateApplication(pid) }
    func hash(_ node: AXUIElement) -> UInt { CFHash(node) }
    func same(_ lhs: AXUIElement, _ rhs: AXUIElement) -> Bool { CFEqual(lhs, rhs) }

    private func check() throws {
        guard deadline.isFinite, now() < deadline else { throw AppUIDriverFailure.deadlineExceeded }
        guard try !cancelled() else { throw AppUIDriverFailure.cancelled }
    }
    private func prepare(_ node: AXUIElement) throws {
        try check()
        var observed: pid_t = 0
        try status(AXUIElementGetPid(node, &observed))
        guard observed == pid else { throw AppUIDriverFailure.identityMismatch }
        let remaining = deadline - now()
        guard remaining > 0 else { throw AppUIDriverFailure.deadlineExceeded }
        try status(AXUIElementSetMessagingTimeout(node, Float(min(0.25, remaining))))
        try check()
    }
    private func status(_ value: AXError) throws {
        guard value == .success else {
            throw AppUIDriverAXError(failure: .axFailure, rawValue: value.rawValue)
        }
    }
    private func attribute(_ node: AXUIElement, _ name: String) throws -> CFTypeRef? {
        try prepare(node)
        var value: CFTypeRef?
        let result = AXUIElementCopyAttributeValue(node, name as CFString, &value)
        if result != .noValue && result != .attributeUnsupported { try status(result) }
        try check()
        if result == .noValue || result == .attributeUnsupported { return nil }
        guard value != nil else { throw AppUIDriverFailure.invalidElement }
        return value
    }
    func related(_ node: AXUIElement, _ name: String) throws -> [AXUIElement] {
        try prepare(node)
        var value: CFArray?
        let result = AXUIElementCopyAttributeValues(node, name as CFString, 0, 10_001, &value)
        if result != .noValue && result != .attributeUnsupported { try status(result) }
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
            try prepare(element)
            nodes.append(element)
        }
        return nodes
    }
    func string(_ node: AXUIElement, _ name: String) throws -> String? {
        guard let value = try attribute(node, name) else { return nil }
        guard CFGetTypeID(value) == CFStringGetTypeID() else { throw AppUIDriverFailure.invalidElement }
        let text = unsafeBitCast(value, to: CFString.self)
        guard CFStringGetLength(text) <= 1_024, let result = value as? String,
              result.utf8.count <= 1_024 else { throw AppUIDriverFailure.protocolLimit }
        return result
    }
    func enabled(_ node: AXUIElement) throws -> Bool {
        guard let value = try attribute(node, kAXEnabledAttribute), CFGetTypeID(value) == CFBooleanGetTypeID()
        else { throw AppUIDriverFailure.invalidElement }
        return CFBooleanGetValue(unsafeBitCast(value, to: CFBoolean.self))
    }
    func press(_ node: AXUIElement) throws {
        try prepare(node)
        let result = AXUIElementPerformAction(node, kAXPressAction as CFString)
        try status(result)
        try check()
    }
    func setSearch(_ node: AXUIElement) throws {
        try prepare(node)
        var settable: DarwinBoolean = false
        let writable = AXUIElementIsAttributeSettable(node, kAXValueAttribute as CFString, &settable)
        try status(writable)
        try check()
        guard settable.boolValue else { throw AppUIDriverFailure.invalidElement }
        try prepare(node)
        let result = AXUIElementSetAttributeValue(node, kAXValueAttribute as CFString, "Indigo" as CFString)
        try status(result)
        try check()
    }
}
