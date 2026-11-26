import XCTest
@testable import BridgeVMControl

final class CreateVMDriverSelectionTests: XCTestCase {
    private let automatic = HvfWindowsDriverPayloadFetch.Prepared(payloadDirectory: "automatic", manifestPath: "automatic.tsv")
    private let failure = HvfWindowsDriverPayloadFetch.Failure(message: "Fixture download failed")

    func testManualChoiceWinsOverAnInFlightAutomaticResult() {
        var selection = CreateVMDriverSelection()
        selection.choosePayload("previous")
        selection.chooseManifest("previous.tsv")
        XCTAssertTrue(selection.beginFetch())
        XCTAssertFalse(selection.ready)
        selection.chooseManifest("manual.tsv")
        XCTAssertTrue(selection.ready)
        XCTAssertFalse(selection.beginFetch(), "The previous worker still owns the cache operation")
        selection.finishFetch(.success(automatic))
        XCTAssertEqual(selection.payloadPath, "previous")
        XCTAssertEqual(selection.manifestPath, "manual.tsv")
        XCTAssertEqual(selection.status, .manual)
        XCTAssertFalse(selection.fetchInFlight)

        XCTAssertTrue(selection.beginFetch())
        selection.choosePayload("previous")
        selection.finishFetch(.failure(failure))
        XCTAssertEqual(selection.status, .manual, "Reselecting the same file is still a newer choice")
        XCTAssertTrue(selection.ready)
    }

    @MainActor
    func testOnlyTheISOInstallWaitsForTheChosenAutomaticDrivers() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        let library = LibraryModel(rootURL: root, migrateLegacy: false, startsModelsAutomatically: false)
        var selection = CreateVMDriverSelection()
        selection.choosePayload("previous")
        selection.chooseManifest("previous.tsv")
        XCTAssertTrue(selection.beginFetch())

        let install = CreateVMSheet(library: library, mode: .windowsHVFInstall, name: "Fixture",
                                    isoPath: "fixture.iso", drivers: selection)
        XCTAssertFalse(install.canCreate)
        install.create()
        XCTAssertFalse(library.preparingNewVM)
        let imported = CreateVMSheet(library: library, mode: .windowsHVF, name: "Fixture",
                                     drivers: selection, hvfTargetPath: "fixture.raw")
        XCTAssertTrue(imported.canCreate)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty)
    }

    func testAutomaticSuccessAndFailureReflectTheDisplayedPair() {
        var selection = CreateVMDriverSelection()
        XCTAssertTrue(selection.beginFetch())
        selection.finishFetch(.failure(failure))
        XCTAssertEqual(selection.status, .failed(failure.message))
        XCTAssertFalse(selection.ready)

        XCTAssertTrue(selection.beginFetch())
        selection.finishFetch(.success(automatic))
        XCTAssertEqual(selection.status, .prepared)
        XCTAssertEqual(selection.payloadPath, automatic.payloadDirectory)
        XCTAssertEqual(selection.manifestPath, automatic.manifestPath)
        XCTAssertTrue(selection.ready)

        XCTAssertTrue(selection.beginFetch())
        XCTAssertFalse(selection.ready)
        selection.finishFetch(.failure(failure))
        XCTAssertEqual(selection.status, .failed(failure.message))
        XCTAssertEqual(selection.payloadPath, automatic.payloadDirectory)
        XCTAssertEqual(selection.manifestPath, automatic.manifestPath)
        XCTAssertTrue(selection.ready)
    }
}
