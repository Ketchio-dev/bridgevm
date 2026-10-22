import ApplicationServices
import Foundation

final class AppUIDriverAXCallsFixture {
    typealias API = AppUIDriverAXContext.API
    // Creating opaque references does not message an application or query AX trust.
    let parent = AXUIElementCreateApplication(2_000_000_001)
    let child = AXUIElementCreateApplication(2_000_000_002)
    var calls: [API] = []
    var fail: API?
    var occurrence = 1
    var error = AXError.illegalArgument
    var observedPID: pid_t = 42
    var time = 100.0
    var cancelled = false
    var afterCall: ((API) -> Void)?
    var array: [AXUIElement] = []
    var reportedCount: CFIndex?
    var value: CFTypeRef? = "fixture" as CFString
    var writable = true
    var ranges: [(CFIndex, CFIndex)] = []
    var timeouts: [Float] = []
    var attributes: [String] = []
    var actions: [String] = []
    var assigned: [String] = []

    func result(_ api: API) -> AXError {
        calls.append(api)
        afterCall?(api)
        return fail == api && calls.filter({ $0 == api }).count == occurrence ? error : .success
    }
    func access(deadline: Double = 105) -> AppUIDriverAX {
        // Replace every system messaging closure, including operations not expected in a test.
        let injected = AppUIDriverAXCalls(
            getPID: { _, output in output.pointee = self.observedPID; return self.result(.getPID) },
            timeout: { _, seconds in self.timeouts.append(seconds); return self.result(.setMessagingTimeout) },
            attribute: { _, name, output in
                self.attributes.append(name as String); output.pointee = self.value
                return self.result(.copyAttributeValue)
            },
            relationships: relationshipCalls(),
            press: { _, action in self.actions.append(action as String); return self.result(.performAction) },
            settable: { _, name, output in
                self.attributes.append(name as String); output.pointee = DarwinBoolean(self.writable)
                return self.result(.isAttributeSettable)
            },
            setValue: { _, name, value in
                self.attributes.append(name as String); self.assigned.append(value as! String)
                return self.result(.setAttributeValue)
            })
        return AppUIDriverAX(pid: 42, deadline: deadline, cancelled: { self.cancelled },
                            calls: injected, now: { self.time })
    }
}
