import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeCLICreateWindowsTests: XCTestCase {
    func testCreatesOwnedPendingRegistrationAndReturnsNoMediaPath() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let request = fixture.request(name: "개발 VM")
        let result = try NativeCLICreateWindows.create(request, libraryRoot: fixture.library)
        let saved = try NativeLibraryReader.readConfig(rootURL: fixture.library, id: result.id)
        let pending = try XCTUnwrap(HvfWindowsInstallRequest.load(bundlePath: saved.bundlePath))

        XCTAssertEqual(result.schema, "bridgevm.app-create-windows.v2")
        XCTAssertEqual(result.id, "개발-vm")
        XCTAssertEqual(result.displayName, "개발 VM")
        XCTAssertEqual(result.backendKind, "hvf-engine")
        XCTAssertEqual(result.bootMode, "windows-hvf")
        XCTAssertTrue(result.installPending)
        XCTAssertTrue(saved.installPending == true)
        XCTAssertEqual(pending.diskGiB, 64)
        // The launch reads guestResolution, not displayWidth/Height: --resolution must land there.
        XCTAssertEqual(saved.guestResolution, "1440x900")
        XCTAssertEqual(HvfGuestResolutionPicker.firmwareMode(saved.guestResolution), "1440x900")
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: pending.isoPath)), fixture.isoBytes)
        XCTAssertNotEqual(pending.isoPath, fixture.iso.path)

        let encoded = String(decoding: try JSONEncoder().encode(result), as: UTF8.self)
        XCTAssertFalse(encoded.contains(fixture.iso.path))
        XCTAssertFalse(encoded.lowercased().contains("isopath"))
        XCTAssertTrue(result.text.contains("bridgevm app inspect \(result.id)"))
        XCTAssertTrue(result.text.contains("No installation or guest boot was run"))
    }

    func testEquivalentNameSpellingsCreateDistinctIDsThatRoundTripThroughInventory() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let name = "Studio 한글"
        let otherSpelling = name.decomposedStringWithCanonicalMapping
        let first = try NativeCLICreateWindows.create(fixture.request(name: name), libraryRoot: fixture.library)
        let second = try NativeCLICreateWindows.create(fixture.request(name: otherSpelling), libraryRoot: fixture.library)
        XCTAssertEqual(Data(first.id.utf8), Data("studio-한글".utf8))
        XCTAssertEqual(Data(second.id.utf8), Data("studio-한글-2".utf8))
        XCTAssertEqual(Data(second.displayName.utf8), Data(otherSpelling.utf8))
        let listed = try NativeLibraryReader.snapshot(rootURL: fixture.library)
        XCTAssertEqual(listed.records.map { Data($0.id.utf8) }, [Data(first.id.utf8), Data(second.id.utf8)])
        for created in [first, second] {
            let alias = created.id.decomposedStringWithCanonicalMapping
            let inspected = try NativeLibraryReader.snapshot(rootURL: fixture.library, id: alias)
            XCTAssertEqual(inspected.records.map { Data($0.id.utf8) }, [Data(created.id.utf8)])
        }
    }

    private struct Fixture {
        let root: URL
        let library: URL
        let iso: URL
        let isoBytes = Data("tiny-owned-iso".utf8)
        init() throws {
            root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
            library = root.appendingPathComponent("library", isDirectory: true)
            iso = root.appendingPathComponent("Windows.iso")
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            try isoBytes.write(to: iso)
        }
        func request(name: String) -> NativeCLICreateWindowsOptions {
            .init(name: name, isoPath: iso.path, diskGiB: 64, memoryMiB: 6_144,
                  cpuCount: 4, resolution: .init(width: 1440, height: 900), networkEnabled: true)
        }
        func remove() { try? FileManager.default.removeItem(at: root) }
    }
}
