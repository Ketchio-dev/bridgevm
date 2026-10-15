import XCTest
@testable import BridgeVMControl

final class LibraryDeletionCaseAliasTests: XCTestCase {
    func testSavedSymlinkUsesFilesystemParentIdentityAcrossCaseSpellings() throws {
        let fm = FileManager.default
        let root = LibraryDeletionTestDirectory.temporary.appendingPathComponent("deletion-alias-" + UUID().uuidString)
        defer { try? fm.removeItem(at: root) }
        let library = root.appendingPathComponent("library")
        let entry = library.appendingPathComponent("owner")
        let bundle = entry.appendingPathComponent("bundle.vmbridge")
        try fm.createDirectory(at: bundle, withIntermediateDirectories: true)
        let iso = root.appendingPathComponent("external.iso")
        try Data("synthetic external installer".utf8).write(to: iso)
        let selected = config("owner", bundle: bundle)
        XCTAssertTrue(VMLibrary.save(selected, rootURL: library))

        let alternate = library.appendingPathComponent(selected.slug.uppercased())
        let aliasesEntry = fm.fileExists(atPath: alternate.path)
        if !aliasesEntry { try fm.createDirectory(at: alternate, withIntermediateDirectories: false) }
        let savedLink = alternate.appendingPathComponent("source-link.iso")
        try fm.createSymbolicLink(at: savedLink, withDestinationURL: iso)
        let bytes = try Data(contentsOf: savedLink)
        let target = try fm.destinationOfSymbolicLink(atPath: savedLink.path)
        var sibling = config("sibling", bundle: root.appendingPathComponent("outside-bundle"))
        sibling.isoPath = savedLink.path
        XCTAssertTrue(VMLibrary.save(sibling, rootURL: library))
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: library, id: sibling.slug), sibling)
        print("deletion-case-alias: root=\(root.path), aliases differently-cased parent=\(aliasesEntry)")
        XCTAssertEqual(LibraryDeletionPathReference.mayReference(
            alternate.appendingPathComponent("missing/descendant").path, removedEntry: entry), aliasesEntry)

        XCTAssertEqual(LibraryDeletionProtection.delete(selected, rootURL: library), !aliasesEntry)
        XCTAssertEqual(fm.fileExists(atPath: entry.path), aliasesEntry)
        XCTAssertEqual(try? Data(contentsOf: savedLink), bytes)
        XCTAssertEqual(try? fm.destinationOfSymbolicLink(atPath: savedLink.path), target)
        XCTAssertEqual(try Data(contentsOf: iso), bytes)
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: library, id: sibling.slug), sibling)
    }

    func testCyclicPathCannotBeProvedIndependentButMissingExternalPathCan() throws {
        let fm = FileManager.default
        let root = LibraryDeletionTestDirectory.temporary.appendingPathComponent("deletion-loop-" + UUID().uuidString)
        defer { try? fm.removeItem(at: root) }
        let entry = root.appendingPathComponent("entry")
        try fm.createDirectory(at: entry, withIntermediateDirectories: true)
        let loop = root.appendingPathComponent("loop")
        try fm.createSymbolicLink(at: loop, withDestinationURL: loop)
        XCTAssertTrue(LibraryDeletionPathReference.mayReference(loop.path, removedEntry: entry))
        XCTAssertFalse(LibraryDeletionPathReference.mayReference(
            root.appendingPathComponent("missing/external").path, removedEntry: entry))
        let outside = root.appendingPathComponent("outside")
        try fm.createDirectory(at: outside.appendingPathComponent("child"), withIntermediateDirectories: true)
        let source = outside.appendingPathComponent("source.iso")
        try Data("external".utf8).write(to: source)
        let link = entry.appendingPathComponent("link")
        try fm.createSymbolicLink(at: link, withDestinationURL: outside.appendingPathComponent("child"))
        let traversal = link.path + "/../source.iso"
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: traversal)), try Data(contentsOf: source))
        XCTAssertTrue(LibraryDeletionPathReference.mayReference(traversal, removedEntry: entry))
    }

    private func config(_ id: String, bundle: URL) -> VMConfig {
        VMConfig(id: id, name: id, displayName: id, backendKind: "fast-vz", bootMode: nil,
            bundlePath: bundle.path, runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
            sshUser: "", leasesPath: "", guestName: id, displayWidth: 800, displayHeight: 600, installPending: false)
    }
}
