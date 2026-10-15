#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

// Diagnostic vocabulary only: never serialize an AX value or an arbitrary attribute name.
struct AppUIDriverAXContext: Codable, Equatable {
    enum API: String, Codable, CaseIterable {
        case getPID, setMessagingTimeout, copyAttributeValue, copyAttributeValues
        case isAttributeSettable, performAction, setAttributeValue
    }
    enum Attribute: String, Codable, CaseIterable {
        case windows = "AXWindows", children = "AXChildren", identifier = "AXIdentifier"
        case role = "AXRole", title = "AXTitle", description = "AXDescription"
        case value = "AXValue", enabled = "AXEnabled", none, other
        init(_ name: String?) { self = name.flatMap(Self.init(rawValue:)) ?? (name == nil ? .none : .other) }
    }
    enum Scope: String, Codable { case element, relatedElement }
    let api: API
    let attribute: Attribute
    let scope: Scope

    func validate(outcome: AppUIDriverOutcome, failure: AppUIDriverFailure?, rawValue: Int32?) throws {
        guard outcome == .refused, failure == .axFailure, let rawValue, rawValue != 0,
              scope != .relatedElement || api == .getPID || api == .setMessagingTimeout else {
            throw AppUIDriverFailure.invalidReply
        }
    }
}
#endif
