#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit
import Darwin

@MainActor
enum AppUIInstallCancelTermination {
    static func finish(_ capture: AppUIInstallCancelCapture, error: Error?, terminate: Bool) async {
        await capture.finish(error: error)
        guard terminate else { return }
        guard let application = NSApp else { exit(2) }
        application.terminate(nil)
    }
}
#endif
