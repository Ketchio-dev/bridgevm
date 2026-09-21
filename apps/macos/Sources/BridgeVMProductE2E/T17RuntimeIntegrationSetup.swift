import Foundation

enum T17RuntimeIntegrationSetup {
    static let hostShareChooser = "bridgevm.runtime.share.host.choose"
    static let guestShare = "C:\\bridgevm-share"

    static func apply(sharePath: String, ui: T17UIControlling) throws {
        try ui.setToggle(true, identifier: "bridgevm.runtime.clipboard", timeout: 10)
        try ui.setToggle(true, identifier: "bridgevm.runtime.network", timeout: 10)
        try ui.setToggle(true, identifier: "bridgevm.runtime.share.enabled", timeout: 10)
        try ui.choose(path: sharePath, from: hostShareChooser, timeout: 20)
        guard try ui.text("bridgevm.runtime.share.guest", timeout: 10) == guestShare else {
            throw T17Blocker(code: "ui-element-missing", detail: "product guest-share default is missing or changed")
        }
    }
}
