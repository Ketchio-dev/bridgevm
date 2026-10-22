#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation
import XCTest
@testable import BridgeVMControl

/// The overview phase of the owned UI host, without an application or window:
/// the seeded rows' control models are counted exactly over an inert backend.
@MainActor
final class AppUIHostOverviewContractTests: XCTestCase {
    func testFixtureCreatesExactlyOneInertModelPerSeededVM() throws {
        let (output, root) = try ownedOutput()
        let capture = try AppUIHostCapture(output: output)
        let library = AppUIHost.makeLibrary(root: root, capture: capture)
        XCTAssertEqual(try report(output)["model_creations"] as? [String: Int], counts(0, 0, 0))
        // Through import, the real first-run action only selects the import form: no row, no model.
        let welcome = try XCTUnwrap(values(FirstRunWelcomeView.self, in: FirstRunView(library: library).body).first)
        welcome.importAction()
        XCTAssertEqual(library.selectedID, LibraryModel.firstRunImportSelectionID)
        XCTAssertTrue(library.cachedControlModels.isEmpty)
        try capture.save()
        XCTAssertEqual(try report(output)["model_creations"] as? [String: Int], counts(0, 0, 0))

        AppUIHost.seedOverview(library, root: root, work: capture.work)
        XCTAssertEqual(library.vms.map(\.slug), ["ui-amber", "ui-indigo"])
        for config in library.vms {
            let model = library.model(for: config) // exactly what each LibraryWorkspaceRow requests
            XCTAssertTrue(model.backend is AppUIHostInertBackend)
            XCTAssertNil(model.timer, "a fixture model must never start polling")
            XCTAssertFalse(model.running)
            for _ in 0..<3 { XCTAssertTrue(library.model(for: config) === model, "re-rendered rows reuse the cache") }
        }
        XCTAssertTrue(capture.work.complete)
        try capture.save()
        let saved = try report(output)
        XCTAssertEqual(saved["model_creations"] as? [String: Int], counts(0, 2, 2))
        XCTAssertEqual(saved["tripwires"] as? [String: Int], ["runtime_creations": 0, "install_creations": 0,
            "file_jobs": 0, "backend_calls": 0])
        XCTAssertTrue(saved["failure"] is NSNull)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), [])

        // The admission behind the fatal factory refusal, on separate accounting.
        let early = AppUIHostDomainWork()
        XCTAssertFalse(early.admit(library.vms[0]), "no model before seeding")
        XCTAssertEqual(early.modelCreations, counts(1, 0, 0))
        XCTAssertFalse(early.complete)
        let late = AppUIHostDomainWork()
        let seeded = late.seed(library.vms)
        XCTAssertTrue(late.admit(seeded[0]))
        XCTAssertFalse(late.complete, "one of two seeded rows is not exact")
        XCTAssertFalse(late.admit(seeded[0]), "never a second model for a seeded VM")
        var unseeded = seeded[1]
        unseeded.id = "ui-unseeded"
        XCTAssertFalse(late.admit(unseeded), "never a model for an unseeded VM")
        XCTAssertTrue(late.admit(seeded[1]))
        XCTAssertEqual(late.modelCreations, counts(0, 4, 2))
        XCTAssertFalse(late.complete)
    }

    private func counts(_ before: Int, _ after: Int, _ seeded: Int) -> [String: Int] {
        ["before_seeding": before, "after_seeding": after, "seeded_fixture_vms": seeded]
    }

    private func ownedOutput() throws -> (output: URL, root: URL) {
        let base = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("bridgevm-ui-host-overview-" + UUID().uuidString, isDirectory: true)
        let output = base.appendingPathComponent("app-ui-private/host-observations", isDirectory: true)
        try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true,
                                               attributes: [.posixPermissions: 0o700])
        addTeardownBlock { try? FileManager.default.removeItem(at: base) }
        let root = output.appendingPathComponent("fixture-library", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false,
                                               attributes: [.posixPermissions: 0o700])
        return (output, root)
    }

    private func report(_ output: URL) throws -> [String: Any] {
        let data = try Data(contentsOf: output.appendingPathComponent("ui-observations.json"))
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let typed = value as? T { return [typed] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }
}
#endif
