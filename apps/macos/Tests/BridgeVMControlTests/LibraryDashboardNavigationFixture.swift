import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryDashboardNavigationFixture {
    let root = LibraryDeletionTestDirectory.temporary.appendingPathComponent("dashboard-navigation-" + UUID().uuidString)
    let library: LibraryModel

    init() throws {
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        library = LibraryModel(rootURL: root, migrateLegacy: false, startsModelsAutomatically: false)
        let config = VMConfig(id: "owned", name: "Owned", displayName: "Owned", backendKind: "fast-vz",
            bootMode: "linux", bundlePath: root.appendingPathComponent("bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: "owned", displayWidth: 1280, displayHeight: 800)
        library.vms = [config]
        library.selectedID = config.slug
    }

    func paletteCommand(_ title: String) throws -> PaletteCommand {
        try XCTUnwrap(values(PaletteCommand.self, in: CommandPaletteView(library: library).body)
            .first { $0.title == title }, "The actual palette must expose the requested navigation action")
    }

    func detailRoute() throws -> LibraryDetailView {
        try XCTUnwrap(values(LibraryDetailView.self, in: LibraryDashboardRoute(library: library).body).first)
    }

    func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let result = value as? T { return [result] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }

    func clean() { try? FileManager.default.removeItem(at: root) }
}
