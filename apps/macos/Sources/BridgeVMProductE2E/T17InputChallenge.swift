import Foundation

/// Host input waits for the form's ready marker, written once the user's
/// desktop takes input on first logon, which can take over a minute.
struct T17InputChallenge {
    let sharePath: String
    let nonce: String
    let ui: T17UIControlling
    var readyTimeout: TimeInterval = 180
    var outputTimeout: TimeInterval = 60

    func deliver() throws {
        let share = URL(fileURLWithPath: sharePath)
        try T17InputChallengeShare.waitUntilShown(share: share, nonce: nonce, timeout: readyTimeout)
        try ui.press("bridgevm.runtime.display.open", timeout: 10)
        // First logon can leave the Start menu over the form: click the form beside it, then its centre.
        for spot in [CGPoint(x: 0.04, y: 0.5), CGPoint(x: 0.5, y: 0.5)] {
            try ui.clickDisplaySurface(at: spot, timeout: 15)
        }
        try ui.fill("t17kbd\(prefix)", identifier: "bridgevm.runtime.keyboard.input", timeout: 10)
        try ui.press("bridgevm.runtime.keyboard.send", timeout: 10)
        do { try T17InputChallengeShare.awaitOutput(share: share, nonce: nonce, timeout: outputTimeout) }
        catch let blocker as T17Blocker {
            throw T17Blocker(code: blocker.code, detail: blocker.detail + ui.displayInputDiagnostic())
        }
    }
    private var prefix: String { String(nonce.prefix(12)) }
}
