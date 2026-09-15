import AppKit
@testable import BridgeVMControl

@MainActor
final class HvfAppUIFixture {
    let root: URL
    let library: LibraryModel
    private let presentation: HvfAppUIWindow
    var window: NSWindow { presentation.window }
    var content: NSView { presentation.content }
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
        presentation = HvfAppUIWindow(rootView: ContentView(library: library))
    }

    func setPresentation(dark: Bool, minimum: Bool) async throws {
        try await presentation.setPresentation(dark: dark, minimum: minimum)
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
        window.contentViewController = nil
        window.close()
        try? FileManager.default.removeItem(at: root)
    }
}
