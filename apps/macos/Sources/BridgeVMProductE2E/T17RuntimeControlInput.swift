import Foundation

/// The guest control input lives in the runtime's collapsed diagnostics group,
/// which SwiftUI omits from the accessibility tree until it is opened.
enum T17RuntimeControlInput {
    static let input = "bridgevm.runtime.ctl.input"
    static let send = "bridgevm.runtime.ctl.send"
    static let opener = "bridgevm.runtime.diagnostics.toggle"

    static func enter(_ command: String, ui: T17UIControlling) throws {
        // The opener toggles, so expand only when the input is not already present.
        if (try? ui.waitFor(input, timeout: 1)) == nil {
            try ui.expand(opener, timeout: 10)
        }
        try ui.setText(command, identifier: input, timeout: 10)
        try ui.press(send, timeout: 10)
    }
}
