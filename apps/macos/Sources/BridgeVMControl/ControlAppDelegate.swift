#if canImport(AppKit)
import AppKit
@MainActor
final class ControlAppDelegate: NSObject, NSApplicationDelegate {
    private lazy var termination = ControlAppTermination(
        sessions: { NativeRuntimeAppOwner.terminationSessions }, presenter: .alerts,
        reply: { NSApp.reply(toApplicationShouldTerminate: $0) })

    func applicationDidFinishLaunching(_ notification: Notification) {
        ControlAppActivation.activate()
        ControlAppPower.shared.failure = {
            let alert = NSAlert()
            alert.messageText = "Mac 잠자기를 막지 못했으니 긴 작업 중에는 Mac의 잠자기 설정을 확인하세요."
            alert.runModal()
        }
        #if DEBUG && BRIDGEVM_APP_UI_HOST
        AppUIHost.prepared?.applicationDidFinishLaunching(notification)
        #endif
    }
    /// Cmd+Q, the Dock, a Mac logout/restart/shutdown and the last window all ask here first.
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        termination.shouldTerminate(reason: .current())
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        let sessions = NativeRuntimeAppOwner.sessions
        let plan = ControlAppReopenPlan.make(hasVisibleWindows: flag,
            runningVMs: sessions.map { $0.hasActiveRuntimeWork },
            visibleDisplays: sessions.map { HvfDisplayWindowController.hasVisibleDisplay(for: $0) })
        if plan.showLibrary { ControlLibraryWindow.show() }
        for index in plan.displayIndices {
            let session = sessions[index]
            HvfDisplayWindowController.present(session: session, title: HvfDisplayWindowTitle.resolve(
                libraryName: session.config.libraryContext?.config.name, targetDiskPath: session.config.targetDiskPath))
        }
        return true
    }
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
    func applicationWillTerminate(_ notification: Notification) {
        ControlAppPower.shared.shutdown()
        NativeRuntimeAppOwner.shutdown()
    }
}
#endif
