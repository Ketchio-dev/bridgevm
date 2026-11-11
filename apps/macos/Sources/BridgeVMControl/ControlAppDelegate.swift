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
    /// Finder's Open With > BridgeVM and files dropped on the Dock icon (CFBundleDocumentTypes,
    /// rank Alternate): copied to the running Windows and opened there (HvfOpenFiles).
    func application(_ application: NSApplication, open urls: [URL]) {
        HvfOpenFiles.open(urls, sessions: NativeRuntimeAppOwner.sessions,
            frontToBack: HvfDisplayWindowController.sessionsFrontToBack(),
            present: { session in
                HvfDisplayWindowController.present(session: session, title: HvfDisplayWindowTitle.resolve(
                    libraryName: session.config.libraryContext?.config.name, targetDiskPath: session.config.targetDiskPath))
            },
            tell: { text in
                // After the open event, so a launch by Open With shows it over the library window.
                DispatchQueue.main.async {
                    let alert = NSAlert()
                    alert.messageText = text
                    alert.runModal()
                }
            })
    }
    /// Runs only after every stop above has completed: releases the library owner lock.
    func applicationWillTerminate(_ notification: Notification) { NativeRuntimeAppOwner.shutdown() }
}
#endif
