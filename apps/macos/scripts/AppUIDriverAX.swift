import ApplicationServices
import Foundation

struct AppUIDriverAX: AppUIDriverAXAccess, AppUIDriverAXPreparation {
    let pid: pid_t
    let deadline: Double
    let cancelled: () throws -> Bool
    var calls = AppUIDriverAXCalls()
    var now: () -> Double = { ProcessInfo.processInfo.systemUptime }

    func application() -> AXUIElement { AXUIElementCreateApplication(pid) }
    func hash(_ node: AXUIElement) -> UInt { CFHash(node) }
    func same(_ lhs: AXUIElement, _ rhs: AXUIElement) -> Bool { CFEqual(lhs, rhs) }

    private func attribute(_ node: AXUIElement, _ name: String) throws -> CFTypeRef? {
        try prepare(node, attribute: name)
        var value: CFTypeRef?
        let result = calls.attribute(node, name as CFString, &value)
        if result != .noValue && result != .attributeUnsupported { try status(result, .copyAttributeValue, attribute: name) }
        try check()
        if result == .noValue || result == .attributeUnsupported { return nil }
        guard value != nil else { throw AppUIDriverFailure.invalidElement }
        return value
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
        let result = calls.press(node, kAXPressAction as CFString)
        try status(result, .performAction)
        try check()
    }
    func setSearch(_ node: AXUIElement) throws {
        try prepare(node, attribute: kAXValueAttribute)
        var settable: DarwinBoolean = false
        let writable = calls.settable(node, kAXValueAttribute as CFString, &settable)
        try status(writable, .isAttributeSettable, attribute: kAXValueAttribute)
        try check()
        guard settable.boolValue else { throw AppUIDriverFailure.invalidElement }
        try prepare(node, attribute: kAXValueAttribute)
        let result = calls.setValue(node, kAXValueAttribute as CFString, "Indigo" as CFString)
        try status(result, .setAttributeValue, attribute: kAXValueAttribute)
        try check()
    }
}
