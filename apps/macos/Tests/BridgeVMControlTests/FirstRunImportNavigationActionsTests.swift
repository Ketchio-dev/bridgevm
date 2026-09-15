import XCTest
@testable import BridgeVMControl

@MainActor
final class FirstRunImportNavigationActionsTests: XCTestCase {
    func testAnotherImportReturnsToFieldsAfterOverviewAndProOff() throws {
        try withLibrary { library, config in
            library.firstRunImport.finish(error: "Owned publication warning", publishedConfig: config)
            library.selectedID = nil
            library.proMode = true
            library.proMode = false // Existing palette route back to the empty-library detail.
            let routed = try XCTUnwrap(values(FirstRunView.self,
                in: LibraryDetailView(library: library).body).first)
            XCTAssertEqual(values(FirstRunImportStatusView.self, in: routed.body).count, 1)

            XCTAssertTrue(library.returnToFirstRunInputs())

            XCTAssertEqual(library.selectedID, LibraryModel.firstRunImportSelectionID)
            XCTAssertFalse(library.proMode)
            XCTAssertNil(library.firstRunImport.publishedConfig)
            XCTAssertNil(library.firstRunImportError)
            let body = FirstRunView(library: library).body
            XCTAssertEqual(values(FirstRunImportFields.self, in: body).count, 1)
            XCTAssertTrue(values(FirstRunWelcomeView.self, in: body).isEmpty)
        }
    }

    func testBusyRecoveryRefusesNavigationAndPreservesPublishedState() throws {
        try withLibrary { library, config in
            library.firstRunImport.finish(error: "Owned warning", publishedConfig: config)
            library.firstRunImport.beginRecovery()
            let operation = library.firstRunImport.operationID
            library.selectedID = "unchanged-selection"
            library.proMode = true

            XCTAssertFalse(library.returnToFirstRunInputs())

            XCTAssertEqual(library.selectedID, "unchanged-selection")
            XCTAssertTrue(library.proMode)
            XCTAssertEqual(library.firstRunImport.operationID, operation)
            XCTAssertEqual(library.firstRunImport.publishedConfig, config)
            XCTAssertEqual(library.firstRunImportError, "Owned warning")
        }
    }

    func testUnpublishedErrorDoesNotResetOrNavigate() throws {
        try withLibrary { library, _ in
            library.firstRunImport.finish(error: "Owned validation failure")
            library.proMode = true
            XCTAssertFalse(library.returnToFirstRunInputs())
            XCTAssertNil(library.selectedID)
            XCTAssertTrue(library.proMode)
            XCTAssertEqual(library.firstRunImportError, "Owned validation failure")
        }
    }

    private func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let typed = value as? T { return [typed] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }

    private func withLibrary(_ body: (LibraryModel, VMConfig) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let library = LibraryModel(rootURL: root, migrateLegacy: false, modelFactory: { config in
            XCTFail("Navigation must not construct a control model")
            return ControlModel(config: config, startsAutomatically: false)
        })
        let config = VMConfig(id: "owned", name: "Owned fixture", displayName: "Owned fixture",
            backendKind: "hvf-engine", bootMode: "windows-hvf", bundlePath: root.appendingPathComponent("uncreated").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
            guestName: "owned", displayWidth: 1280, displayHeight: 800)
        try body(library, config)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty)
    }
}
