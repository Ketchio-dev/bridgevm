import Foundation

enum T17RuntimeIntegrationSetup {
    static let hostShareChooser = "bridgevm.runtime.share.host.choose"
    static let guestShare = "C:\\bridgevm-share"

    static func apply(sharePath: String, bundle: URL, ui: T17UIControlling) throws {
        try T17SettingsSheet.edit(ui, section: T17SettingsSheet.sharing) {
            try ui.setToggle(false, identifier: "bridgevm.runtime.clipboard", timeout: 10)
            try ui.setToggle(true, identifier: "bridgevm.runtime.share.enabled", timeout: 10)
            try ui.choose(path: sharePath, from: hostShareChooser, timeout: 20)
            guard try ui.text("bridgevm.runtime.share.guest", timeout: 10) == guestShare else {
                throw T17Blocker(code: "ui-element-missing", detail: "product guest-share default is missing or changed")
            }
            try ui.press(T17SettingsSheet.network, timeout: 10)
            try ui.setToggle(true, identifier: "bridgevm.runtime.network", timeout: 10)
        }
        let vmDirectory = bundle.deletingLastPathComponent()
        let config = vmDirectory.appendingPathComponent("vm.json")
        guard T17BoundedLog.regularFile(config),
              let data = try? Data(contentsOf: config),
              let registration = try? JSONDecoder().decode(Registration.self, from: data),
              registration.id == vmDirectory.lastPathComponent, registration.bundlePath == bundle.path,
              !registration.clipboardSyncEnabled else {
            throw T17Blocker(code: "ui-element-missing", detail: "Mac clipboard sync must be disabled in the lane VM registration before Start")
        }
    }

    private struct Registration: Decodable {
        let id: String
        let bundlePath: String
        let clipboardSyncEnabled: Bool
    }
}
