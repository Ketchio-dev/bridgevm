import Foundation

/// Finder's Open With > BridgeVM and files dropped on the Dock icon (ControlAppDelegate): the files
/// go to the running Windows like a drop on its window (copyDroppedFiles), and once they are
/// copied each one opens there with its default Windows app (file_drop.rs, `BVDROP ... open`).
@MainActor
enum HvfOpenFiles {
    static let noWindowsText = "Start a Windows VM, then open the selection again."
    static let whichWindowsText = "Several Windows VMs are running. Click the window of the one that should get the files, then open them again."

    /// The only running VM, else the running one whose display window is frontmost.
    static func target(sessions: [HvfEngineSession], frontToBack: [HvfEngineSession]) -> HvfEngineSession? {
        let running = sessions.filter(\.runsWindows)
        if running.count == 1 { return running[0] }
        return frontToBack.first { front in running.contains { $0 === front } }
    }

    /// Shows the target VM's window and copies `urls` there to be opened, or says in one sentence
    /// why nothing happens. Returns the staging work so tests can wait for it.
    @discardableResult
    static func open(_ urls: [URL], sessions: [HvfEngineSession], frontToBack: [HvfEngineSession],
                     present: (HvfEngineSession) -> Void, tell: (String) -> Void) -> Task<Void, Never>? {
        let files = urls.filter(\.isFileURL)
        guard !files.isEmpty else { return nil }
        guard let session = target(sessions: sessions, frontToBack: frontToBack) else {
            tell(sessions.contains(where: \.runsWindows) ? whichWindowsText : noWindowsText)
            return nil
        }
        present(session)
        return session.copyDroppedFiles(files, open: true)
    }
}

extension HvfEngineSession {
    /// Booting or running: files opened now go to this VM (while it boots, its window says the
    /// guest agent is not ready yet).
    var runsWindows: Bool {
        switch connectionState {
        case .booting, .connected: return true
        case .stopped, .stopping, .timedOut: return false
        }
    }
}
