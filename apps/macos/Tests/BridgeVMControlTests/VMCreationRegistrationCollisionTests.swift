import Foundation
import XCTest
@testable import BridgeVMControl

final class VMCreationRegistrationCollisionTests: XCTestCase {
    struct Fixture {
        let root: URL
        var library: URL { root.appendingPathComponent("library") }
        var storage: URL { root.appendingPathComponent("external-storage") }
        var guiISO: URL { root.appendingPathComponent("gui.iso") }
        var cliISO: URL { root.appendingPathComponent("cli.iso") }
        let guiBytes = Data("owned-gui-iso".utf8)
        let cliBytes = Data("owned-cli-iso".utf8)

        init() throws {
            let base = ProcessInfo.processInfo.environment["BRIDGEVM_STORAGE_RECOVERY_FIXTURE_ROOT"]
                .map { URL(fileURLWithPath: $0, isDirectory: true) }
                ?? FileManager.default.temporaryDirectory
            root = base.appendingPathComponent("creation-registration-" + UUID().uuidString)
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
            try guiBytes.write(to: guiISO)
            try cliBytes.write(to: cliISO)
        }

        func createExternal(
            persist: Bool = true,
            publish: (VMConfig, URL) -> VMRegistrationCommitOutcome = { VMLibrary.createOutcome($0, rootURL: $1) }
        ) -> HVFCreationOutcome? {
            VMLibrary.createWindowsHVFInstall(name: "Collision", isoPath: guiISO.path,
                diskGiB: 64, injectViogpu3d: false, driverPackageDir: nil,
                storageDir: storage, libraryRoot: library, persist: persist, publish: publish)
        }

        func createCLI() throws -> VMConfig {
            let request = NativeCLICreateWindowsOptions(name: "Collision", isoPath: cliISO.path,
                diskGiB: 64, memoryMiB: 4096, cpuCount: 2,
                resolution: .init(width: 1280, height: 800), networkEnabled: false)
            let output = try NativeCLICreateWindows.create(request, libraryRoot: library)
            return try NativeLibraryReader.readConfig(rootURL: library, id: output.id)
        }

        func ownedISO(_ config: VMConfig) throws -> Data {
            let request = try XCTUnwrap(HvfWindowsInstallRequest.load(bundlePath: config.bundlePath))
            return try Data(contentsOf: URL(fileURLWithPath: request.isoPath))
        }

        func remove() { try? FileManager.default.removeItem(at: root) }
    }

    func testOrdinaryExternalCreationPublishesItsOwnRegistration() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        guard case .created(let config)? = fixture.createExternal() else {
            return XCTFail("ordinary external creation must succeed")
        }
        XCTAssertTrue(config.bundlePath.hasPrefix(fixture.storage.path + "/"))
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: fixture.library, id: config.slug), config)
        XCTAssertEqual(try fixture.ownedISO(config), fixture.guiBytes)
        XCTAssertEqual(try Data(contentsOf: fixture.guiISO), fixture.guiBytes)
    }

    func testExternalCreationPreservesRegistrationPublishedDuringItsCopy() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        var second: VMConfig?
        var publishedBytes: Data?
        // GUI custom-storage creation copies before publishing. A CLI process
        // can create the same requested name in the default library meanwhile.
        // The existing seam deterministically holds that real publication gap.
        let outcome = fixture.createExternal { config, root in
            do {
                let saved = try fixture.createCLI()
                second = saved
                publishedBytes = try Data(contentsOf: root.appendingPathComponent(saved.slug + "/vm.json"))
                return VMLibrary.createOutcome(config, rootURL: root)
            } catch {
                XCTFail("second actual creation failed: \(error)")
                return .notPublished(error)
            }
        }
        let saved = try XCTUnwrap(second)
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: fixture.library, id: saved.slug), saved,
            "earlier external reservation must not replace the later committed VM")
        XCTAssertEqual(try Data(contentsOf: fixture.library.appendingPathComponent(saved.slug + "/vm.json")),
            try XCTUnwrap(publishedBytes), "committed registration bytes must remain unchanged")
        XCTAssertEqual(try fixture.ownedISO(saved), fixture.cliBytes)
        XCTAssertEqual(try Data(contentsOf: fixture.guiISO), fixture.guiBytes)
        XCTAssertEqual(try Data(contentsOf: fixture.cliISO), fixture.cliBytes)
        if case .created(let external)? = outcome {
            XCTAssertEqual(try fixture.ownedISO(external), fixture.guiBytes)
            XCTAssertNotEqual(external.slug, saved.slug, "two successful creations need distinct registrations")
        }
    }

    /// Every Create failure said "Couldn’t create the VM or save it to the library."; the ones the user can
    /// act on now say what to do.
    func testCreateFailuresTheUserCanFixSayWhatToDo() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        func create(iso: URL, storage: URL) -> HVFCreationOutcome? {
            VMLibrary.createWindowsHVFInstall(name: "Fails", isoPath: iso.path, diskGiB: 64, injectViogpu3d: false,
                driverPackageDir: nil, storageDir: storage, libraryRoot: fixture.library)
        }
        guard case .refused(let ejected)? = create(iso: fixture.root.appendingPathComponent("ejected.iso"),
                                                     storage: fixture.storage) else { return XCTFail("missing ISO") }
        XCTAssertEqual(ejected, CreateFailure.isoUnreadable)

        let locked = fixture.root.appendingPathComponent("locked")
        try FileManager.default.createDirectory(at: locked, withIntermediateDirectories: false)
        try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: locked.path)
        defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: locked.path) }
        let storage = locked.appendingPathComponent("VMs")
        guard case .refused(let folder)? = create(iso: fixture.guiISO, storage: storage) else { return XCTFail("folder") }
        XCTAssertEqual(folder, CreateFailure.folder(storage))

        XCTAssertNil(CreateFailure.isoSpace(isoBytes: 6 << 30, sameVolume: true, freeBytes: 0, volume: "Mac"),
                     "a clone on the same volume needs no space")
        XCTAssertNil(CreateFailure.isoSpace(isoBytes: 6 << 30, sameVolume: false, freeBytes: 8 << 30, volume: "USB"))
        XCTAssertEqual(CreateFailure.isoSpace(isoBytes: 6 << 30, sameVolume: false, freeBytes: 5 << 30, volume: "USB"),
            "“USB” doesn’t have enough free space to copy the Windows ISO (about 7 GB). Free up space, or choose another location.")
    }

    func testRefusedDefaultCreatorKeepsWinnerPublishedInsideItsReservedDirectory() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        var preparedLoser: VMConfig?
        var winnerBytes: Data?
        let outcome = fixture.createExternal { winner, root in
            var publication: VMRegistrationCommitOutcome = .notPublished(CocoaError(.fileWriteUnknown))
            let loser = VMLibrary.createWindowsHVFInstall(name: "Collision", isoPath: fixture.cliISO.path,
                diskGiB: 64, injectViogpu3d: false, driverPackageDir: nil, libraryRoot: root,
                publish: { pending, root in
                    preparedLoser = pending
                    publication = VMLibrary.createOutcome(winner, rootURL: root)
                    winnerBytes = try? Data(contentsOf: root.appendingPathComponent(winner.slug + "/vm.json"))
                    return VMLibrary.createOutcome(pending, rootURL: root)
                })
            // A late conflicting registration refuses publication, and says so (it was the general sentence).
            guard case .refused(let message)? = loser else { XCTFail("expected a refusal"); return publication }
            XCTAssertTrue(message.contains("BridgeVM kept the files it prepared"), message)
            return publication
        }
        guard case .created(let winner)? = outcome else { return XCTFail("expected real winner publication") }
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: fixture.library, id: winner.slug), winner)
        XCTAssertEqual(try Data(contentsOf: fixture.library.appendingPathComponent(winner.slug + "/vm.json")),
            try XCTUnwrap(winnerBytes))
        XCTAssertEqual(try fixture.ownedISO(winner), fixture.guiBytes)
        // Cleanup conservatively leaves this incomplete bundle beside the
        // winner's registration because the directory is no longer solely ours.
        XCTAssertEqual(try fixture.ownedISO(XCTUnwrap(preparedLoser)), fixture.cliBytes)
        XCTAssertEqual(try Data(contentsOf: fixture.guiISO), fixture.guiBytes)
        XCTAssertEqual(try Data(contentsOf: fixture.cliISO), fixture.cliBytes)
    }
}
