import XCTest
@testable import BridgeVMControl

@MainActor
final class FirstRunPresentationTests: XCTestCase {
    func testEmptyLibraryOffersWelcomeWithoutImportFieldsOrStatus() throws {
        try withLibrary { library in
            let body = FirstRunView(library: library).body
            XCTAssertEqual(values(FirstRunWelcomeView.self, in: body).count, 1)
            XCTAssertTrue(values(FirstRunImportFields.self, in: body).isEmpty)
            XCTAssertTrue(values(FirstRunImportStatusView.self, in: body).isEmpty)
        }
    }

    func testWelcomeCreateActionOpensExistingCreationSheet() throws {
        try withLibrary { library in
            let welcome = try XCTUnwrap(values(FirstRunWelcomeView.self,
                in: FirstRunView(library: library).body).first)
            XCTAssertFalse(library.showingCreate)
            welcome.createAction()
            XCTAssertTrue(library.showingCreate)
            XCTAssertNil(library.selectedID)
            XCTAssertFalse(library.firstRunImportBusy)
        }
    }

    func testWelcomeImportActionLeavesProModeAndSelectsImportFields() throws {
        try withLibrary { library in
            library.proMode = true
            let welcome = try XCTUnwrap(values(FirstRunWelcomeView.self,
                in: FirstRunView(library: library).body).first)
            welcome.importAction()
            XCTAssertEqual(library.selectedID, LibraryModel.firstRunImportSelectionID)
            XCTAssertFalse(library.proMode)
            XCTAssertFalse(library.showingCreate)
            assertImportPresentation(library, editable: true)
        }
    }

    func testActiveImportStagesAlwaysKeepStatusAndHideEditableFields() throws {
        try withLibrary { library in
            let operation = library.firstRunImport.begin()
            let stages: [FirstRunImportStage] = [.validating, .copying, .saving, .readingLibrary]
            for stage in stages {
                library.firstRunImport.advance(stage, operation: operation)
                XCTAssertNil(library.selectedID, "Busy state must preserve progress without a route sentinel")
                XCTAssertTrue(library.firstRunImportBusy)
                XCTAssertEqual(library.firstRunImport.stage, stage)
                assertImportPresentation(library, editable: false)
            }
        }
    }

    func testFailedImportKeepsRetryFieldsAndStatusWithoutImportSelection() throws {
        try withLibrary { library in
            library.firstRunImport.finish(error: "Owned test validation refusal")
            XCTAssertNil(library.selectedID)
            XCTAssertFalse(library.firstRunImportBusy)
            assertImportPresentation(library, editable: true)
        }
    }

    func testPublishedImportKeepsRecoveryStatusWithoutEditableFields() throws {
        try withLibrary { library in
            let config = VMConfig(id: "owned-unadopted", name: "Owned unadopted", displayName: "Owned",
                backendKind: "hvf-engine", bootMode: "windows-hvf",
                bundlePath: library.rootURL.appendingPathComponent("uncreated.vmbridge").path,
                runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                leasesPath: "", guestName: "owned", displayWidth: 1280, displayHeight: 800)
            let warnings: [String?] = [nil, "Owned test read-back refusal"]
            for warning in warnings {
                library.firstRunImport.finish(error: warning, publishedConfig: config)
                XCTAssertNil(library.selectedID)
                XCTAssertFalse(library.firstRunImportBusy)
                assertImportPresentation(library, editable: false)
            }
            library.firstRunImport.beginRecovery()
            XCTAssertTrue(library.firstRunImportBusy)
            assertImportPresentation(library, editable: false)
        }
    }

    private func assertImportPresentation(_ library: LibraryModel, editable: Bool,
        file: StaticString = #filePath, line: UInt = #line) {
        let body = FirstRunView(library: library).body
        XCTAssertTrue(values(FirstRunWelcomeView.self, in: body).isEmpty, file: file, line: line)
        XCTAssertEqual(values(FirstRunImportFields.self, in: body).count, editable ? 1 : 0,
            file: file, line: line)
        let status = values(FirstRunImportStatusView.self, in: body)
        XCTAssertEqual(status.count, 1, file: file, line: line)
        XCTAssertTrue(status.first?.library === library, file: file, line: line)
    }

    // Inspect composed values only: no child body evaluation or class graph traversal.
    private func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let result = value as? T { return [result] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }

    private func withLibrary(_ body: (LibraryModel) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let library = LibraryModel(rootURL: root, migrateLegacy: false, modelFactory: { config in
            XCTFail("First-run presentation must not create a VM control model")
            return ControlModel(config: config, startsAutomatically: false)
        })
        try body(library)
        XCTAssertTrue(library.vms.isEmpty)
        XCTAssertTrue(library.runningModels().isEmpty)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty)
    }
}
