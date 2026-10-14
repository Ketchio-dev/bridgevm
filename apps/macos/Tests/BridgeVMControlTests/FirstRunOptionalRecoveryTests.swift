import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

@MainActor
final class FirstRunOptionalRecoveryTests: XCTestCase {
    private typealias UI = FirstRunOptionalRecoveryFixture

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
