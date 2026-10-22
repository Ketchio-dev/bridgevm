import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class RetainedTerminalRecoveryFixture {
    let root: URL, registration: URL
    let original: VMConfig
    let library: LibraryModel
    let factory: RetainedTerminalRecoveryFactory
    let accepted: ControlModel

    init(kind: String = "fast-vz") throws {
        let temporary = try XCTUnwrap(ProcessInfo.processInfo.environment["TMPDIR"])
        let ownedRoot = URL(fileURLWithPath: temporary).resolvingSymlinksInPath()
            .appendingPathComponent("retained-terminal-" + UUID().uuidString)
        root = ownedRoot
        guard mkdir(ownedRoot.path, 0o700) == 0 else { throw CocoaError(.fileWriteFileExists) }
        var prepared = false
        defer { if !prepared { try? FileManager.default.removeItem(at: ownedRoot) } }
        let config = VMConfig(id: "current", name: "Original", displayName: "Original",
            backendKind: kind, bootMode: kind == "fast-vz" ? "direct-kernel" : "windows-uefi",
            bundlePath: root.appendingPathComponent("absent-bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: "owned", displayWidth: 1280, displayHeight: 720)
        guard VMLibrary.save(config, rootURL: root) else { throw CocoaError(.fileWriteUnknown) }
        original = try NativeLibraryReader.readConfig(rootURL: root, id: config.slug)
        registration = root.appendingPathComponent(config.slug + "/vm.json")
        let factory = RetainedTerminalRecoveryFactory()
        self.factory = factory
        library = LibraryModel(rootURL: root, migrateLegacy: false, startsModelsAutomatically: false,
            modelFactory: { factory.make($0) })
        accepted = library.model(for: original)
        prepared = true
    }

    deinit { try? FileManager.default.removeItem(at: root) }

    func replaceWhileObservedRunning() throws -> (VMConfig, LibraryRetainedControlRecord) {
        accepted.running = true // Synthetic observed state; no runtime Start, adoption, or process lookup.
        var current = original
        current.name = "Current registration"; current.displayName = "Current registration"
        guard VMLibrary.save(current, rootURL: root) else { throw CocoaError(.fileWriteUnknown) }
        library.reload()
        XCTAssertEqual(library.vms, [current])
        XCTAssertTrue(library.libraryIssues.isEmpty)
        XCTAssertTrue(library.model(for: current) === accepted)
        XCTAssertTrue(accepted.isRetained)
        let record = try XCTUnwrap(library.retainedControlRecords.first)
        XCTAssertEqual(record.descriptor.config, original)
        return (current, record)
    }

    func retainedView(_ record: LibraryRetainedControlRecord) throws -> RetainedGenericControlView {
        let navigation = try XCTUnwrap(values(LibraryRetainedControlsNavigation.self,
            in: LibrarySidebar(library: library).body).first)
        navigation.selection.wrappedValue = record.id
        let detail = try XCTUnwrap(values(LibraryDetailView.self,
            in: LibraryDashboardRoute(library: library).body).first)
        let retained = try XCTUnwrap(values(RetainedControlDetailView.self, in: detail.body).first)
        let view = try XCTUnwrap(values(RetainedGenericControlView.self, in: retained.body).first)
        XCTAssertTrue(view.model === accepted)
        return view
    }

    func ordinaryControls() throws -> LibraryDashboardLegacyControls {
        let dashboard = try XCTUnwrap(values(LibraryDashboardDetail.self,
            in: LibraryDashboardRoute(library: library).body).first)
        XCTAssertEqual(dashboard.config, library.vms.first)
        return try XCTUnwrap(values(LibraryDashboardLegacyControls.self, in: dashboard.body).first)
    }

    func start(_ controls: LibraryDashboardLegacyControls) async throws {
        let action = try XCTUnwrap(values(DashboardActionButton.self, in: controls.body)
            .first { $0.title == "시작" })
        XCTAssertFalse(action.disabled)
        action.action()
        let deadline = ProcessInfo.processInfo.systemUptime + 3
        while controls.model.lifecycleBusy, ProcessInfo.processInfo.systemUptime < deadline {
            try await Task.sleep(nanoseconds: 5_000_000)
        }
        XCTAssertFalse(controls.model.lifecycleBusy)
    }

    func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let typed = value as? T { return [typed] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }
}
