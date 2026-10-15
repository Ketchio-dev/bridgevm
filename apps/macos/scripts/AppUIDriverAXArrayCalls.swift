import ApplicationServices

// Both array messages are injectable; deterministic tests never contact an app.
struct AppUIDriverAXArrayCalls {
    var count: (AXUIElement, CFString, UnsafeMutablePointer<CFIndex>) -> AXError = {
        AXUIElementGetAttributeValueCount($0, $1, $2)
    }
    var read: (AXUIElement, CFString, CFIndex, CFIndex, UnsafeMutablePointer<CFArray?>) -> AXError = {
        AXUIElementCopyAttributeValues($0, $1, $2, $3, $4)
    }
}
