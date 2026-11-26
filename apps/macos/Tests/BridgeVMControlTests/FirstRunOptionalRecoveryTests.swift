import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

@MainActor
final class FirstRunOptionalRecoveryTests: XCTestCase {
    private typealias UI = FirstRunOptionalRecoveryFixture

    func testEscapeLeavesAnUnsubmittedDraftWithoutChangingTheLibrary() async throws {
        try await UI.run { fixture, library, host in
            let escape = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [],
                timestamp: 0, windowNumber: 0, context: nil, characters: "\u{1b}",
                charactersIgnoringModifiers: "\u{1b}", isARepeat: false, keyCode: 53))
            XCTAssertTrue(host.performKeyEquivalent(with: escape))
            await UI.settle(host)
            XCTAssertTrue(UI.fields(host).isEmpty)
            XCTAssertNil(library.selectedID)
            XCTAssertNil(library.firstRunImport.stage)
            XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path))
        }
    }

    func testClearingStateOmitsHiddenRecoveryDraftsFromActualImport() async throws {
        try await UI.run { fixture, library, host in
            _ = try await self.selectRecoveryDrafts(fixture, host: host)
            UI.edit(try XCTUnwrap(UI.fields(host).dropFirst(3).first), to: "")
            await UI.settle(host)
            XCTAssertEqual(UI.fields(host).count, 4, "Inactive recovery drafts have no visible fields")
            try await UI.submit(host, library: library)
            XCTAssertNil(library.firstRunImportError,
                "Hidden recovery drafts must not block importing without optional vTPM state")
            guard library.firstRunImportError == nil else { return }
            try fixture.assertPersisted()
            XCTAssertEqual(library.selectedID, library.vms.first?.slug)
        }
    }

    func testReselectingStateRestoresDraftsAndStillValidatesRecoveryFiles() async throws {
        try await UI.run { fixture, library, host in
            let paths = try await self.selectRecoveryDrafts(fixture, host: host)
            UI.edit(try XCTUnwrap(UI.fields(host).dropFirst(3).first), to: "")
            await UI.settle(host)
            XCTAssertEqual(UI.fields(host).count, 4)
            UI.edit(try XCTUnwrap(UI.fields(host).last), to: paths.state.path)
            await UI.settle(host)
            let restored = UI.fields(host)
            XCTAssertEqual(restored.count, 6)
            guard restored.count == 6 else { return }
            XCTAssertEqual(restored[4].stringValue, paths.package.path)
            XCTAssertEqual(restored[5].stringValue, paths.code.path)
            try await UI.submit(host, library: library)
            XCTAssertEqual(library.firstRunImportError,
                FirstRunImport.ValidationError.vtpmRecoveryMissing(paths.package.path).description)
            XCTAssertTrue(library.vms.isEmpty)
            XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path))
        }
    }

    func testStateRequiresBothRecoveryPathsBeforeReturnCanSubmit() async throws {
        try await UI.run { fixture, library, host in
            let state = fixture.root.appendingPathComponent("state")
            try FileManager.default.createDirectory(at: state, withIntermediateDirectories: false)
            UI.edit(try XCTUnwrap(UI.fields(host).last), to: state.path)
            await UI.settle(host)
            let fields = UI.fields(host)
            XCTAssertEqual(fields.count, 6)
            guard fields.count == 6 else { return }

            _ = try UI.pressReturn(host)
            await UI.settle(host)
            XCTAssertNil(library.firstRunImport.stage, "Missing recovery package must keep the form open")
            guard library.firstRunImport.stage == nil else { return }

            UI.edit(fields[4], to: fixture.disk.path)
            UI.edit(fields[5], to: "  ")
            await UI.settle(host)
            _ = try UI.pressReturn(host)
            await UI.settle(host)
            XCTAssertNil(library.firstRunImport.stage, "Missing recovery code must keep the form open")
            guard library.firstRunImport.stage == nil else { return }

            let absent = fixture.root.appendingPathComponent("missing-code").path
            UI.edit(fields[5], to: absent)
            await UI.settle(host)
            try await UI.submit(host, library: library)
            XCTAssertEqual(library.firstRunImportError, FirstRunImport.ValidationError.vtpmRecoveryMissing(absent).description)
        }
    }

    func testMissingRecoverySelectionNamesTheRequiredFile() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        var inputs = fixture.inputs()
        inputs.vtpmStateDir = fixture.root.path
        XCTAssertEqual(FirstRunImport.validate(inputs)?.description, "vTPM 복구 패키지를 선택하세요.")
        inputs.vtpmRecoveryPackagePath = fixture.disk.path
        inputs.vtpmRecoveryCodePath = "  "
        XCTAssertEqual(FirstRunImport.validate(inputs)?.description, "vTPM 복구 코드 파일을 선택하세요.")
    }

    func testInitiallyOmittedStateStillImportsThroughActualDefaultAction() async throws {
        try await UI.run { fixture, library, host in
            XCTAssertEqual(UI.fields(host).count, 4)
            try await UI.submit(host, library: library)
            XCTAssertNil(library.firstRunImportError)
            guard library.firstRunImportError == nil else { return }
            try fixture.assertPersisted()
        }
    }

    func testGlobalValidatorStillRejectsExplicitRecoveryWithoutState() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        var inputs = fixture.inputs()
        inputs.vtpmRecoveryPackagePath = fixture.root.appendingPathComponent("absent-package").path
        inputs.vtpmRecoveryCodePath = fixture.root.appendingPathComponent("absent-code").path
        XCTAssertEqual(FirstRunImport.validate(inputs), .vtpmRecoveryWithoutState)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path))
    }

    private func selectRecoveryDrafts(_ fixture: FirstRunImportTransactionFixture,
        host: NSView) async throws -> (state: URL, package: URL, code: URL) {
        let state = fixture.root.appendingPathComponent("optional-state")
        let package = fixture.root.appendingPathComponent("retained-package")
        let code = fixture.root.appendingPathComponent("retained-code")
        try FileManager.default.createDirectory(at: state, withIntermediateDirectories: false)
        UI.edit(try XCTUnwrap(UI.fields(host).last), to: state.path)
        await UI.settle(host)
        let selected = UI.fields(host)
        XCTAssertEqual(selected.count, 6)
        guard selected.count == 6 else { throw CocoaError(.fileReadUnknown) }
        UI.edit(selected[4], to: package.path)
        UI.edit(selected[5], to: code.path)
        await UI.settle(host)
        XCTAssertFalse(FileManager.default.fileExists(atPath: package.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: code.path))
        return (state, package, code)
    }
}
