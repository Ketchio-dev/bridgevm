import AppKit
import SwiftUI
@testable import BridgeVMControl

@MainActor
final class HvfAppUIFixture {
    let root: URL
    let library: LibraryModel
    let window: NSWindow
    let content: NSHostingView<ContentView>
    let capture: HvfAppUICapture

    init(capture: HvfAppUICapture) throws {
        self.capture = capture
        root = FileManager.default.temporaryDirectory.appendingPathComponent("bridgevm-ui-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        library = LibraryModel(rootURL: root, migrateLegacy: false,
            installSessionFactory: { _ in capture.tripwire("install_creations") },
            runtimeSessionFactory: { _ in capture.tripwire("runtime_creations") },
            actionScheduler: { _ in capture.tripwire("file_jobs") }, startsModelsAutomatically: false,
            modelFactory: { _ in capture.tripwire("model_creations") })
        content = NSHostingView(rootView: ContentView(library: library))
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1320, height: 860),
            styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        window.title = "BridgeVM — owned UI diagnostic"
        window.isReleasedWhenClosed = false
        window.contentMinSize = NSSize(width: 1100, height: 720)
        window.contentView = content
        window.appearance = NSAppearance(named: .aqua)
        window.makeKeyAndOrderFront(nil)
    }

    func setPresentation(dark: Bool, minimum: Bool) async throws {
        window.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        window.setContentSize(NSSize(width: minimum ? 1100 : 1320, height: minimum ? 720 : 860))
        try await Task.sleep(nanoseconds: 200_000_000)
        content.layoutSubtreeIfNeeded()
        guard abs(content.bounds.width - (minimum ? 1100 : 1320)) < 1,
              abs(content.bounds.height - (minimum ? 720 : 860)) < 1,
              content.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == (dark ? .darkAqua : .aqua)
        else { throw HvfAppUIError.refused("Owned content did not adopt the requested dimensions and appearance") }
    }

    func seedOverviewMetadata() {
        // Explicitly synthetic registration metadata only. No registration or
        // media exists on disk, and no VM card is ever activated by this test.
        library.vms = [("Amber", 4096), ("Indigo", 8192)].map { name, memory in
            VMConfig(id: "ui-" + name.lowercased(), name: name, displayName: name,
                backendKind: "hvf-engine", bootMode: "windows-hvf",
                bundlePath: root.appendingPathComponent(name + ".vmbridge").path,
                runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                leasesPath: "", guestName: "owned-ui-fixture", displayWidth: 1280, displayHeight: 800,
                memMiB: memory, cpuCount: 4)
        }
    }

    func assertNoLibraryWrites() throws {
        guard try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty
        else { throw HvfAppUIError.refused("UI-only fixture unexpectedly wrote to its library") }
    }

    func close() {
        for sheet in window.sheets { window.endSheet(sheet); sheet.close() }
        window.contentView = nil
        window.close()
        try? FileManager.default.removeItem(at: root)
    }
}
