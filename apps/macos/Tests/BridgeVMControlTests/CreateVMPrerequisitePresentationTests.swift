import SwiftUI
import XCTest
@testable import BridgeVMControl

@MainActor
final class CreateVMPrerequisitePresentationTests: XCTestCase {
    func testWindowsModesStartWithoutCreationWarning() throws {
        try withLibrary { library in
            for mode: CreateVMSheet.Mode in [.windowsHVF, .windowsHVFInstall] {
                let sheet = CreateVMSheet(library: library, mode: mode)
                let footer = try footer(of: sheet)
                XCTAssertFalse(footer.canCreate)
                XCTAssertEqual(footer.error, "")
                XCTAssertEqual(footer.failureCode, "")
            }
        }
    }

    func testExistingCreationErrorRemainsVisibleBeforePrerequisiteGuidance() throws {
        try withLibrary { library in
            let sheet = CreateVMSheet(library: library, mode: .windowsHVFInstall,
                error: "Owned publication refusal", creationFailureCode: "vm-publication-unconfirmed")
            let footer = try footer(of: sheet)
            XCTAssertEqual(footer.error, "Owned publication refusal")
            XCTAssertEqual(footer.failureCode, "vm-publication-unconfirmed")
        }
    }

    func testWorkingAndPublishedRecoveryDoNotShowPrerequisiteGuidance() throws {
        try withLibrary { library in
            library.preparingNewVM = true
            let working = try footer(of: CreateVMSheet(library: library, mode: .windowsHVFInstall))
            library.preparingNewVM = false
            XCTAssertFalse(working.canCreate)
            XCTAssertEqual(working.error, "")
            var recovery = CreateVMCreationState()
            _ = recovery.complete(.publishedButUnsynced(config, "Owned warning")) { _ in false }
            let footer = try footer(of: CreateVMSheet(library: library, mode: .windowsHVFInstall, creationState: recovery))
            XCTAssertTrue(footer.recoveryAvailable)
            XCTAssertEqual(footer.error, "")
            XCTAssertEqual(footer.failureCode, "")
        }
    }

    private var config: VMConfig {
        VMConfig(id: "owned", name: "Owned", displayName: "Owned", backendKind: "hvf-engine",
            bootMode: "direct-kernel", bundlePath: "/owned/bundle", runnerPath: "", launchSpecPath: "",
            handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "owned",
            displayWidth: 1280, displayHeight: 800)
    }

    private func footer(of sheet: CreateVMSheet) throws -> CreateVMCreationFooter {
        try XCTUnwrap(values(CreateVMCreationFooter.self, in: sheet.body).first)
    }

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
        let library = LibraryModel(rootURL: root, migrateLegacy: false, startsModelsAutomatically: false)
        try body(library)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty)
    }
}
