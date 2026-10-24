#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

extension AppUIDriverAXContext {
    /// The attribute vocabulary: every other attribute name is recorded only as `other`.
    enum Attribute: String, Codable, CaseIterable {
        case windows = "AXWindows", children = "AXChildren", identifier = "AXIdentifier"
        case role = "AXRole", title = "AXTitle", description = "AXDescription"
        case value = "AXValue", enabled = "AXEnabled", none, other
        init(_ name: String?) { self = name.flatMap(Self.init(rawValue:)) ?? (name == nil ? .none : .other) }
    }
}
#endif
