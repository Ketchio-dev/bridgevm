#if canImport(AppKit)
import AppKit
@MainActor
final class ControlAppDelegate: NSObject, NSApplicationDelegate {
    private lazy var termination = ControlAppTermination(
        sessions: { NativeRuntimeAppOwner.terminationSessions }, presenter: .alerts,
        reply: { NSApp.reply(toApplicationShouldTerminate: $0) })

    func applicationDidFinishLaunching(_ notification: Notification) {
        ControlAppActivation.activate()
        #if DEBUG && BRIDGEVM_APP_UI_HOST
        AppUIHost.prepared?.applicationDidFinishLaunching(notification)
        #endif
    }
    /// Cmd+Q, the Dock, a Mac logout/restart/shutdown and the last window all ask here first.
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        termination.shouldTerminate(reason: .current())
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
    /// Runs only after every stop above has completed: releases the library owner lock.
    func applicationWillTerminate(_ notification: Notification) { NativeRuntimeAppOwner.shutdown() }
}
#endif
