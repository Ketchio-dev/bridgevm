import SwiftUI
#if os(macOS)
import Darwin
#endif
#if canImport(AppKit)
import AppKit

/// Ensure the window appears and takes focus when launched as a SwiftPM
/// executable (no .app bundle), rather than starting as a background agent.
final class ControlAppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}
#endif

@main
enum BridgeVMControlMain {
    static func main() {
        let arguments = Array(CommandLine.arguments.dropFirst())
        if arguments.first == "--vtpm-lifecycle" {
            exit(VTPMLifecycleCommand.run(arguments: Array(arguments.dropFirst())))
        }
        BridgeVMControlApp.main()
    }
}

struct BridgeVMControlApp: App {
#if canImport(AppKit)
    @NSApplicationDelegateAdaptor(ControlAppDelegate.self) private var appDelegate
#endif
    @StateObject private var library = LibraryModel()

    var body: some Scene {
        WindowGroup("BridgeVM Control") {
            ContentView(library: library)
                .frame(minWidth: 1100, minHeight: 720)
        }
        .windowStyle(.titleBar)
        .defaultSize(width: 1320, height: 860)
    }
}
