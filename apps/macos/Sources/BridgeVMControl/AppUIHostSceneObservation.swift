import SwiftUI

extension View {
    @MainActor @ViewBuilder
    func appUIHostSceneObservation() -> some View {
        #if DEBUG && BRIDGEVM_APP_UI_HOST
        background {
            AppUIHostWindow().frame(width: 0, height: 0).allowsHitTesting(false).accessibilityHidden(true)
        }
        .onAppear { AppUIHost.prepared?.lifecycle.record(.rootContentAppeared) }
        #else
        background {}
        #endif
    }
}

#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit
import Darwin

@MainActor
extension AppUIHostLifecycle {
    static func libraryState() -> StateObject<LibraryModel> {
        AppUIHost.prepared?.lifecycle.record(.appInitialization)
        return StateObject(wrappedValue: libraryModel())
    }
    private static func libraryModel() -> LibraryModel {
        guard let host = AppUIHost.prepared else { fatalError("Diagnostic host was not prepared") }
        host.lifecycle.record(.libraryFactory)
        return host.libraryForApplication()
    }
    static func makeAttachmentView() -> NSView {
        AppUIHost.prepared?.lifecycle.record(.representableMake)
        return AppUIHostWindow.AttachmentView(frame: .zero)
    }
}

@MainActor
enum AppUIHostSceneObservation {
    static func saveWindows(_ capture: AppUIHostCapture) {
        let application = NSApp
        let windows = application?.windows ?? []
        let observations = windows.prefix(32).map { window in
            ["is_panel": window is NSPanel, "is_sheet": window.sheetParent != nil,
             "visible": window.isVisible, "has_content": window.contentView != nil]
        }
        do {
            try capture.write(["schema_version": 1, "kind": "native-app-ui-host-own-windows",
                "pid": Int(getpid()), "observed_uptime": ProcessInfo.processInfo.systemUptime,
                "application_exists": application != nil, "window_count": windows.count,
                "observed_count": observations.count, "truncated": windows.count > observations.count,
                "windows": observations], name: "host-own-windows.json", final: true)
        } catch { capture.failed(AppUIHostError.refused("Private own-window receipt could not be saved")) }
    }
}
#endif
