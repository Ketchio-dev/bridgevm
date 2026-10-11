import XCTest
@testable import BridgeVMControl

/// The deletion dialog must not promise to keep a bundle that deleting the
/// library entry removes. A managed bundle whose saved absolute path went stale
/// (renamed home folder, remounted volume) still lives inside the entry.
final class VMLibraryDeletionImpactTests: XCTestCase {
    func testManagedBundleBehindAStaleSavedPathIsReportedAsDeleted() throws {
        let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: temp) }
        let root = temp.appendingPathComponent("library", isDirectory: true)
        for engine in ["fast-vz", "hvf-engine"] {
            let slug = engine == "fast-vz" ? "fast" : "win"
            let bundle = root.appendingPathComponent("\(slug)/bundle.vmbridge", isDirectory: true)
            try FileManager.default.createDirectory(at: bundle, withIntermediateDirectories: true)
            var stale = VMConfig(id: slug, name: slug, displayName: slug, backendKind: engine,
                                 bootMode: nil, bundlePath: temp.appendingPathComponent("old-home/library/\(slug)/bundle.vmbridge").path,
                                 runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                                 leasesPath: "", guestName: slug, displayWidth: 1, displayHeight: 1)
            stale.installPending = false
            XCTAssertTrue(VMLibrary.save(stale, rootURL: root))
            XCTAssertEqual(VMLibrary.deletionImpact(for: stale, rootURL: root), .managedBundleDeleted, engine)
        }
    }

    func testExternalBundleEntryWithoutManagedBundleStaysRegistrationOnly() throws {
        let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: temp) }
        let root = temp.appendingPathComponent("library", isDirectory: true)
        let external = temp.appendingPathComponent("external/ext/bundle.vmbridge", isDirectory: true)
        try FileManager.default.createDirectory(at: external, withIntermediateDirectories: true)
        let config = VMConfig(id: "ext", name: "ext", displayName: "ext", backendKind: "hvf-engine",
                              bootMode: nil, bundlePath: external.path, runnerPath: "", launchSpecPath: "",
                              handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "ext",
                              displayWidth: 1, displayHeight: 1)
        XCTAssertTrue(VMLibrary.save(config, rootURL: root))
        XCTAssertEqual(VMLibrary.deletionImpact(for: config, rootURL: root), .registrationOnly)
    }
}
