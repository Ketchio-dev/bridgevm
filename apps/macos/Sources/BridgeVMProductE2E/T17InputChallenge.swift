import Foundation

/// The launcher replies once the guest process exists, but the challenge counts
/// clicks and keys only after its form is shown, so host input waits for the
/// form's nonce-bound ready marker.
struct T17InputChallenge {
    let sharePath: String
    let nonce: String
    let ui: T17UIControlling
    var readyTimeout: TimeInterval = 60

    func deliver() throws {
        try waitUntilShown()
        try ui.press("bridgevm.runtime.display.open", timeout: 10)
        try ui.clickSecondaryWindow(timeout: 15)
        try ui.fill("t17kbd\(prefix)", identifier: "bridgevm.runtime.keyboard.input", timeout: 10)
        try ui.press("bridgevm.runtime.keyboard.send", timeout: 10)
    }

    private func waitUntilShown() throws {
        let marker = URL(fileURLWithPath: sharePath).appendingPathComponent("t17-keyboard-pointer-ready-\(prefix).txt")
        let body = Data("bridgevm-t17-keyboard-pointer-ready-v1\n\(nonce)\n".utf8)
        let deadline = Date().addingTimeInterval(readyTimeout)
        while !Self.holds(marker, exactly: body) {
            guard Date() < deadline else {
                let present = (try? FileManager.default.attributesOfItem(atPath: marker.path)) != nil
                throw T17Blocker(code: "guest-evidence-missing", detail: present
                    ? "guest input challenge ready marker is unsafe or changed" : "guest input challenge was not shown")
            }
            RunLoop.current.run(until: Date().addingTimeInterval(0.2))
        }
    }

    private static func holds(_ url: URL, exactly body: Data) -> Bool {
        guard let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey]),
              values.isRegularFile == true, values.isSymbolicLink != true else { return false }
        return (try? Data(contentsOf: url)) == body
    }

    private var prefix: String { String(nonce.prefix(12)) }
}
