import SwiftUI
import XCTest
@testable import BridgeVMControl

@MainActor
final class CreateVMPrerequisitePresentationTests: XCTestCase {
    func testDefaultMemoryIsAnAvailableChoiceOnEightGiBAndLargerMacs() {
        for (hostGiB, expectedMiB) in [(8, 4096), (16, 6144)] {
            let memory = CreateVMMemoryOptions(hostBytes: UInt64(hostGiB) << 30)
            XCTAssertEqual(memory.recommendedMiB, expectedMiB)
            XCTAssertTrue(memory.choicesMiB.contains(memory.recommendedMiB))
        }
    }

    func testWindowsModesStartWithoutCreationWarning() throws {
        try withLibrary { library in
            for (mode, hint): (CreateVMSheet.Mode, String) in [
                (.windowsHVF, "Choose the Windows disk to continue."),
                (.windowsHVFInstall, "Choose the Windows 11 for Arm ISO to continue."),
            ] {
                let sheet = CreateVMSheet(library: library, mode: mode)
                let footer = try footer(of: sheet)
                XCTAssertFalse(footer.canCreate)
                XCTAssertEqual(footer.hint, hint, "a greyed-out Create says what it needs")
                XCTAssertEqual(footer.error, "")
                XCTAssertEqual(footer.failureCode, "")
            }
        }
    }

    /// With an ISO but no drivers, Create was greyed out and nothing said why.
    func testAnISOWithoutDriversSaysDriversAreNeeded() throws {
        try withLibrary { library in
            let footer = try footer(of: CreateVMSheet(library: library, isoPath: "Windows.iso"))
            XCTAssertFalse(footer.canCreate)
            XCTAssertEqual(footer.hint, "Choose Download Drivers, or your driver folder and its manifest, to continue.")
        }
    }

    func testReadyMediaUsesAnEditableWindowsNameAndRequiresAReplacementWhenCleared() throws {
        try withLibrary { library in
            var drivers = CreateVMDriverSelection()
            drivers.choosePayload("drivers")
            drivers.chooseManifest("manifest.tsv")
            let install = CreateVMSheet(library: library, isoPath: "Windows.iso", drivers: drivers)
            XCTAssertEqual(install.name, "Windows 11")
            XCTAssertTrue(install.canCreate)
            let cleared = try footer(of: CreateVMSheet(library: library, name: "",
                                                       isoPath: "Windows.iso", drivers: drivers))
            XCTAssertEqual(cleared.hint, CreateVMCreationFooter.nameHint)
            XCTAssertFalse(cleared.canCreate)

            let disk = try footer(of: CreateVMSheet(library: library, mode: .windowsHVF,
                                                   name: "  ", hvfTargetPath: "Windows.raw"))
            XCTAssertEqual(disk.hint, CreateVMCreationFooter.nameHint)
            XCTAssertFalse(disk.canCreate)
            let named = try footer(of: CreateVMSheet(library: library, mode: .windowsHVF,
                                                    name: "연구실 Windows 東京", hvfTargetPath: "Windows.raw"))
            XCTAssertNil(named.hint)
            XCTAssertTrue(named.canCreate)
        }
    }

    func testExistingCreationErrorRemainsVisibleBeforePrerequisiteGuidance() throws {
        try withLibrary { library in
            let sheet = CreateVMSheet(library: library, mode: .windowsHVFInstall,
                error: "Owned publication refusal", creationFailureCode: "vm-publication-unconfirmed")
            let footer = try footer(of: sheet)
            XCTAssertEqual(footer.error, "Owned publication refusal")
            XCTAssertEqual(footer.failureCode, "vm-publication-unconfirmed")
            XCTAssertEqual(CreateVMCreationFooter.errorIdentifier(footer.failureCode),
                           "bridgevm.create.error.vm-publication-unconfirmed")
            XCTAssertEqual(CreateVMCreationFooter.errorIdentifier(""), "bridgevm.create.error")
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
