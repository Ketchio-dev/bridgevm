import Foundation

@MainActor
extension HvfEngineSession {
    @discardableResult
    func sendToolbarKey(_ action: String) -> Bool {
        sendDisplayInput(.key(action))
    }
}
