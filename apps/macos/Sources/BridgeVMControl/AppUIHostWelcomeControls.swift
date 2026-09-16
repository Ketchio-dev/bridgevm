#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit

@MainActor
enum AppUIHostWelcomeControls {
    static func wait(_ host: AppUIHost, window: NSWindow, content: NSView) async throws {
        try await AppUIHostAccessibility.wait("welcome controls", onTimeout: {
            try AppUIHostOwnedViewObservation.saveTimeout(host, window: window, content: content)
        }) {
            try AppUIHostAccessibility.find("bridgevm.first-run.create", in: content) != nil
                && AppUIHostAccessibility.find("bridgevm.first-run.import", in: content) != nil
        }
    }
}

extension AppUIHostAccessibility {
    static func wait(_ reason: String, onTimeout: (() throws -> Void)? = nil,
                     until condition: () throws -> Bool) async throws {
        let deadline = Date().addingTimeInterval(5)
        repeat {
            if try condition() { return }
            try await Task.sleep(nanoseconds: 50_000_000)
        } while Date() < deadline
        try timeout(reason, onTimeout: onTimeout)
    }

    static func timeout(_ reason: String, onTimeout: (() throws -> Void)?) throws -> Never {
        do { try onTimeout?() }
        catch { FileHandle.standardError.write(Data("App UI welcome accessibility observation failed.\n".utf8)) }
        throw AppUIHostError.refused("Timed out waiting for actual UI: \(reason)")
    }
}
#endif
