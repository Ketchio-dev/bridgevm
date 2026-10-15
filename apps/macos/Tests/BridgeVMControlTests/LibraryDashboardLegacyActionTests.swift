import SwiftUI
import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryDashboardLegacyActionTests: XCTestCase {
    func testRunningFastVZDoesNotOfferUnsupportedWindowAction() throws {
        try checkRunning(engine: "fast-vz")
    }

    func testRunningQemuDoesNotOfferUnsupportedWindowAction() throws {
        try checkRunning(engine: "qemu-compat")
    }

    func testStoppedGenericVMKeepsStartAndAdvancedActions() throws {
        for engine in ["fast-vz", "qemu-compat"] {
            try withFixture(engine: engine) { library, model, backend in
                let buttons = try actions(library)
                let start = try XCTUnwrap(buttons.first { $0.prominent })
                XCTAssertEqual(start.title, "시작")
                XCTAssertFalse(start.disabled)
                XCTAssertTrue(try XCTUnwrap(buttons.first { $0.title == "정지" }).disabled)
                XCTAssertFalse(try XCTUnwrap(buttons.first { $0.title == "새로고침" }).disabled)
                XCTAssertFalse(try XCTUnwrap(buttons.first { $0.title == "고급" }).disabled)
                XCTAssertFalse(model.hasActiveRuntimeWork)
                XCTAssertEqual(backend.calls.starts, 0)
            }
        }
    }

    func testAcceptedGenericWorkKeepsStartUnavailable() throws {
        for engine in ["fast-vz", "qemu-compat"] {
            try withFixture(engine: engine) { library, model, backend in
                model.busy = true
                XCTAssertTrue(model.hasActiveRuntimeWork)
                XCTAssertTrue(try XCTUnwrap(actions(library).first { $0.prominent }).disabled)
                model.busy = false
                XCTAssertFalse(try XCTUnwrap(actions(library).first { $0.prominent }).disabled)
                XCTAssertEqual(backend.calls.starts, 0)
            }
        }
    }

    private func checkRunning(engine: String) throws {
        try withFixture(engine: engine) { library, model, backend in
            model.running = true // Synthetic observation only; no process is launched or adopted.
            model.statusNote = "실행 중"
            let buttons = try actions(library)
            let primary = try XCTUnwrap(buttons.first { $0.prominent })
            primary.action() // Existing start ownership guard must still reject duplicate work.
            XCTAssertEqual(backend.calls.starts, 0)
            XCTAssertEqual(model.statusNote, "실행 중")
            XCTAssertTrue(model.running)
            XCTAssertFalse(model.lifecycleBusy)
            XCTAssertEqual(primary.title, "실행 중")
            XCTAssertTrue(primary.disabled, "An unsupported window action must not be offered as actionable")
            XCTAssertFalse(try XCTUnwrap(buttons.first { $0.title == "정지" }).disabled)
        }
    }

    private func actions(_ library: LibraryModel) throws -> [DashboardActionButton] {
        let dashboard = try XCTUnwrap(values(LibraryDashboardDetail.self,
            in: LibraryDashboardRoute(library: library).body).first)
        let controls = try XCTUnwrap(values(LibraryDashboardLegacyControls.self, in: dashboard.body).first)
        return values(DashboardActionButton.self, in: controls.body)
    }

    private func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let result = value as? T { return [result] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }

    private func withFixture(engine: String,
        _ body: (LibraryModel, ControlModel, HvfRuntimeLibraryActionBackend) throws -> Void) throws {
        let root = LibraryDeletionTestDirectory.temporary.appendingPathComponent("legacy-action-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let config = VMConfig(id: "owned", name: "Owned", displayName: "Owned", backendKind: engine,
            bootMode: "direct-kernel", bundlePath: root.appendingPathComponent("bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: "owned", displayWidth: 1280, displayHeight: 800)
        let backend = HvfRuntimeLibraryActionBackend()
        let library = LibraryModel(rootURL: root, migrateLegacy: false, startsModelsAutomatically: false,
            modelFactory: { ControlModel(config: $0, backend: backend, startsAutomatically: false) })
        library.vms = [config]
        library.selectedID = config.slug
        try body(library, library.model(for: config), backend)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty)
    }
}
