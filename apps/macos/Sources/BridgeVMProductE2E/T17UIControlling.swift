import Foundation

protocol T17UIControlling {
    func press(_ identifier: String, timeout: TimeInterval) throws
    func expand(_ identifier: String, timeout: TimeInterval) throws
    func setText(_ value: String, identifier: String, timeout: TimeInterval) throws
    func setToggle(_ enabled: Bool, identifier: String, timeout: TimeInterval) throws
    func choose(path: String, from identifier: String, timeout: TimeInterval) throws
    func waitFor(_ identifier: String, timeout: TimeInterval) throws
    func text(_ identifier: String, timeout: TimeInterval) throws -> String
    func optionalTexts(_ identifiers: Set<String>) throws -> [String: String]
    func clickSecondaryWindow(timeout: TimeInterval) throws
    func textSnapshot() -> [String]
}
