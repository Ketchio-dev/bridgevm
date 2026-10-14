import XCTest
@testable import BridgeVMControl

final class LibraryDeletionReferenceBoundaryTests: XCTestCase {
    func testInstalledDeletionRechecksSavedReferencesAfterLeaseReady() throws {
        for requestOnly in [false, true] {
            let f = try HvfPendingDeletionFixture()
            let installed = try install(f)
            var sibling = try createSibling(in: f)
            let disk = f.bundle.appendingPathComponent("disks/hvf-target.raw")
            let vars = f.bundle.appendingPathComponent("metadata/hvf-vars.fd")
            let diskBytes = try Data(contentsOf: disk)
            let varsBytes = try Data(contentsOf: vars)
            let replacement = f.root.appendingPathComponent("replacement.json")
            let destination: URL
            if requestOnly {
                destination = URL(fileURLWithPath: sibling.bundlePath)
                    .appendingPathComponent(HvfWindowsInstallRequest.fileName)
                var request = try HvfWindowsInstallRequestSnapshot.load(destination).request
                request.isoPath = f.bundle.appendingPathComponent("disks/installer.iso").path
                try JSONEncoder().encode(request).write(to: replacement)
            } else {
                destination = f.library.appendingPathComponent(sibling.slug).appendingPathComponent("vm.json")
                sibling.diskPath = disk.path
                try JSONEncoder().encode(sibling).write(to: replacement)
            }

            // The sibling initially owns unrelated inputs. Publish its reference
            // only when the synthetic ordinary lease is about to report READY.
            XCTAssertNil(LibraryDeletionReferences.refusal(for: installed, rootURL: f.library))
            XCTAssertFalse(try f.delete(installed,
                beforeReady: "/bin/cp '\(replacement.path)' '\(destination.path)'"), "requestOnly=\(requestOnly)")
            XCTAssertTrue(try String(contentsOf: f.command).hasPrefix("lease\n"))
            XCTAssertEqual(try Data(contentsOf: destination), try Data(contentsOf: replacement))
            XCTAssertEqual(try? Data(contentsOf: disk), diskBytes)
            XCTAssertEqual(try? Data(contentsOf: vars), varsBytes)
            XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: f.library, id: installed.slug), installed)
        }
    }

    func testInstalledDeletionWithUnrelatedSiblingStillCompletesOrdinaryLease() throws {
        let f = try HvfPendingDeletionFixture()
        let installed = try install(f)
        let sibling = try createSibling(in: f)
        let source = URL(fileURLWithPath: sibling.bundlePath).appendingPathComponent("disks/installer.iso")
        let sourceBytes = try Data(contentsOf: source)

        XCTAssertTrue(try f.delete(installed))
        XCTAssertTrue(try String(contentsOf: f.command).hasPrefix("lease\n"))
        XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
        XCTAssertEqual(try Data(contentsOf: source), sourceBytes)
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: f.library, id: sibling.slug), sibling)
    }

    private func install(_ f: HvfPendingDeletionFixture) throws -> VMConfig {
        var installed = f.config
        installed.installPending = false
        try f.save(installed)
        try f.file("disks/hvf-target.raw")
        try f.file("metadata/hvf-vars.fd")
        return installed
    }

    private func createSibling(in f: HvfPendingDeletionFixture) throws -> VMConfig {
        _ = try XCTUnwrap(VMLibrary.createWindowsHVFInstall(name: "Sibling", isoPath: f.iso.path,
            diskGiB: 64, injectViogpu3d: false, driverPackageDir: nil, libraryRoot: f.library))
        return try NativeLibraryReader.readConfig(rootURL: f.library, id: "sibling")
    }
}
