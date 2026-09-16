import Foundation

struct AppUIDriverAXError: Error {
    let failure: AppUIDriverFailure
    let rawValue: Int32?
}

protocol AppUIDriverAXAccess {
    associatedtype Element
    func application() -> Element
    func related(_ node: Element, _ attribute: String) throws -> [Element]
    func string(_ node: Element, _ attribute: String) throws -> String?
    func enabled(_ node: Element) throws -> Bool
    func press(_ node: Element) throws
    func setSearch(_ node: Element) throws
    func hash(_ node: Element) -> UInt
    func same(_ lhs: Element, _ rhs: Element) -> Bool
}

enum AppUIDriverAXAttribute {
    static let children = "AXChildren", windows = "AXWindows"
    static let identifier = "AXIdentifier", role = "AXRole", title = "AXTitle"
    static let description = "AXDescription", value = "AXValue"
}

struct AppUIDriverAXResult {
    let outcome: AppUIDriverOutcome
    let values: AppUIDriverValues?
}
