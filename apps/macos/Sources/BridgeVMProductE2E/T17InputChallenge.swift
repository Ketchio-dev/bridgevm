import Foundation

/// The launcher replies once the guest process exists, but the challenge counts
/// clicks and keys only once its form can take input, so host input waits for
/// the form's nonce-bound ready marker. On a first logon that can take over a
/// minute, until Windows switches to the user's desktop.
struct T17InputChallenge {
    let sharePath: String
    let nonce: String
    let ui: T17UIControlling
    let runLog: URL
    var readyTimeout: TimeInterval = 180
    var outputTimeout: TimeInterval = 60

    func deliver() throws {
        let share = URL(fileURLWithPath: sharePath)
        try T17InputChallengeShare.waitUntilShown(share: share, nonce: nonce, timeout: readyTimeout)
        try ui.press("bridgevm.runtime.display.open", timeout: 10)
        try T17PointerReceipt.require(runLog, timeout: 15) { try ui.clickDisplaySurface(timeout: 15) }
        try ui.fill("t17kbd\(prefix)", identifier: "bridgevm.runtime.keyboard.input", timeout: 10)
        try ui.press("bridgevm.runtime.keyboard.send", timeout: 10)
        try T17InputChallengeShare.awaitOutput(share: share, nonce: nonce, timeout: outputTimeout)
    }

    private var prefix: String { String(nonce.prefix(12)) }
}
