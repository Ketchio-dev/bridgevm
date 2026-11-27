#if canImport(AppKit)
import AppKit

/// The drive with a running VM's disk was unmounted (unplugged, or ejected by force). Windows then
/// only gets disk errors, and the run's log and display files were on that drive too, so nothing
/// from the VM can say so: macOS's unmount notice is what tells the app.
@MainActor
enum HvfDriveDisconnect {
    static let text = "The drive with this VM’s disk was disconnected, so Windows can’t use its disk. Choose Force Stop, reconnect the drive, then start Windows again."
    private static var observer: NSObjectProtocol?
    private static var sessions: @MainActor () -> [HvfEngineSession] = { [] }

    static func watch(_ sessions: @escaping @MainActor () -> [HvfEngineSession]) {
        self.sessions = sessions
        guard observer == nil else { return }
        observer = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didUnmountNotification, object: nil, queue: .main
        ) { note in
            guard let volume = note.userInfo?[NSWorkspace.volumeURLUserInfoKey] as? URL else { return }
            MainActor.assumeIsolated {
                for session in HvfDriveDisconnect.sessions() { session.volumeUnmounted(volume) }
            }
        }
    }
}

extension HvfEngineSession {
    func volumeUnmounted(_ volume: URL) {
        guard hasActiveRuntimeWork, !driveDisconnected else { return }
        let root = volume.standardizedFileURL.path
        let disk = URL(fileURLWithPath: config.targetDiskPath).standardizedFileURL.path
        guard root != "/", disk.hasPrefix(root.hasSuffix("/") ? root : root + "/") else { return }
        driveDisconnected = true
    }
}
#endif
