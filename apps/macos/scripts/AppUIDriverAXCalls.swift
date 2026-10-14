import ApplicationServices

// Only this table calls the system messaging APIs; deterministic fixtures replace every closure.
struct AppUIDriverAXCalls {
    var getPID: (AXUIElement, UnsafeMutablePointer<pid_t>) -> AXError = { AXUIElementGetPid($0, $1) }
    var timeout: (AXUIElement, Float) -> AXError = { AXUIElementSetMessagingTimeout($0, $1) }
    var attribute: (AXUIElement, CFString, UnsafeMutablePointer<CFTypeRef?>) -> AXError = {
        AXUIElementCopyAttributeValue($0, $1, $2)
    }
    var related: (AXUIElement, CFString, CFIndex, CFIndex, UnsafeMutablePointer<CFArray?>) -> AXError = {
        AXUIElementCopyAttributeValues($0, $1, $2, $3, $4)
    }
    var press: (AXUIElement, CFString) -> AXError = { AXUIElementPerformAction($0, $1) }
    var settable: (AXUIElement, CFString, UnsafeMutablePointer<DarwinBoolean>) -> AXError = {
        AXUIElementIsAttributeSettable($0, $1, $2)
    }
    var setValue: (AXUIElement, CFString, CFTypeRef) -> AXError = { AXUIElementSetAttributeValue($0, $1, $2) }
}
