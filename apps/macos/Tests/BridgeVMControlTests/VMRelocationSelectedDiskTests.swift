import CryptoKit
import XCTest
@testable import BridgeVMControl

final class VMRelocationSelectedDiskTests: XCTestCase {
    func testImplicitBundledSelectionRemainsImplicitAfterMove() throws { try check(.implicit) }
    func testBuiltinSelectedDiskRemainsReadableAfterMove() throws { try check(.builtin) }
    func testIndependentExternalDiskRemainsReadableAfterMove() throws { try check(.external) }
    func testExternalAliasToBundledDiskRemainsReadableAfterMove() throws { try check(.aliasBuiltin) }
    func testExternalAliasToOtherInternalSelectedDiskRemainsReadableAfterMove() throws { try check(.aliasOther) }
    func testAncestorAliasRebasesCanonicalInternalSelectedDisk() throws { try check(.ancestor) }
    func testExternalAliasToOutsideDiskKeepsSavedPathAndTarget() throws { try check(.aliasExternal) }

    func testExternalAliasMoveCompletesUnderRealNativeLease() throws { try check(.aliasBuiltin, nativeLease: true) }

    private enum Selection { case implicit, builtin, external, aliasBuiltin, aliasOther, ancestor, aliasExternal }
    private func check(_ selection: Selection, nativeLease: Bool = false) throws {
        let fm = FileManager.default
        let root = LibraryDeletionTestDirectory.temporary.appendingPathComponent("move-selection-" + UUID().uuidString)
        defer { try? fm.removeItem(at: root) }
        let source = root.appendingPathComponent("source.bundle")
        let builtin = source.appendingPathComponent("disks/hvf-target.raw")
        let other = source.appendingPathComponent("data/selected.raw")
        let vars = source.appendingPathComponent("metadata/hvf-vars.fd")
        // Shared name prefix is not path-component containment.
        let external = root.appendingPathComponent("source.bundle-other/external.raw")
        let alias = root.appendingPathComponent("selected-alias.raw")
        let ancestor = root.appendingPathComponent("ancestor")
        let library = root.appendingPathComponent("library")
        let parent = root.appendingPathComponent("moved")
        let diskBytes = Data("owned bundled source disk".utf8)
        let otherBytes = Data("owned other internal disk".utf8)
        let externalBytes = Data("owned independent external disk".utf8)
        let varsBytes = Data("owned vars".utf8)
        for (file, bytes) in [(builtin, diskBytes), (other, otherBytes), (vars, varsBytes), (external, externalBytes)] {
            try fm.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
            try bytes.write(to: file)
        }
        let aliasTarget = selection == .aliasOther ? other : selection == .aliasExternal ? external : builtin
        try fm.createSymbolicLink(at: alias, withDestinationURL: aliasTarget)
        try fm.createSymbolicLink(at: ancestor, withDestinationURL: root)
        let selected: URL
        switch selection {
        case .implicit, .builtin, .ancestor: selected = builtin
        case .external: selected = external
        case .aliasBuiltin, .aliasOther, .aliasExternal: selected = alias
        }
        let outside = selection == .external || selection == .aliasExternal
        let expected = outside ? externalBytes : selection == .aliasOther ? otherBytes : diskBytes
        let bundle = selection == .ancestor ? ancestor.appendingPathComponent("source.bundle") : source
        let config = VMConfig(id: "source", name: "Source", displayName: "Source", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: bundle.path, runnerPath: "", launchSpecPath: "", handoffPath: "",
            sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "source", displayWidth: 800, displayHeight: 600,
            diskPath: selection == .implicit ? nil : selected.path)
        XCTAssertTrue(VMLibrary.migrateLegacyIfNeeded(rootURL: library, legacy: config))
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: library, id: "source"), config)
        XCTAssertEqual(try Data(contentsOf: selected), expected)
        let beforeHashes = try hashes([builtin, other, vars, external])
        let lease: HvfMediaLeaseSession?
        if nativeLease {
            lease = try HvfMediaLeaseSession(executable: HvfMediaImportTestSupport.helper,
                disk: selected.path, vars: vars.path)
        } else { lease = nil }
        defer { lease?.abort() }
        // No vTPM state: same-identity move writes a receipt without key-store access.
        let moved = try XCTUnwrap(VMLibrary.moveWindowsHVFBundle(config, to: parent, rootURL: library))
        try lease?.finish()
        let runtime = try XCTUnwrap(HvfEngineConfig.libraryVM(moved, rootURL: library))
        let destination = URL(fileURLWithPath: moved.bundlePath)
        XCTAssertEqual(runtime.targetDiskPath, moved.diskPath ?? destination.appendingPathComponent("disks/hvf-target.raw").path)
        XCTAssertEqual(try? Data(contentsOf: URL(fileURLWithPath: runtime.targetDiskPath)), expected)
        let relative = selection == .aliasOther ? "data/selected.raw" : "disks/hvf-target.raw"
        XCTAssertEqual(moved.diskPath, selection == .implicit ? nil : outside ? selected.path : destination.appendingPathComponent(relative).path)
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: library, id: "source"), moved)
        XCTAssertEqual(try Data(contentsOf: destination.appendingPathComponent("disks/hvf-target.raw")), diskBytes)
        XCTAssertEqual(try Data(contentsOf: destination.appendingPathComponent("data/selected.raw")), otherBytes)
        XCTAssertEqual(try Data(contentsOf: destination.appendingPathComponent("metadata/hvf-vars.fd")), varsBytes)
        let afterHashes = try hashes([destination.appendingPathComponent("disks/hvf-target.raw"),
            destination.appendingPathComponent("data/selected.raw"), destination.appendingPathComponent("metadata/hvf-vars.fd"), external])
        XCTAssertEqual(beforeHashes, afterHashes)
        if nativeLease { print("relocation-native-lease-source-sha256 before=\(beforeHashes) after=\(afterHashes)") }
        XCTAssertFalse(fm.fileExists(atPath: source.path))
        XCTAssertFalse(VMRelocationJournal.isPending(config, rootURL: library))
        XCTAssertEqual(try Data(contentsOf: external), externalBytes)
        XCTAssertEqual(try fm.destinationOfSymbolicLink(atPath: alias.path), aliasTarget.path)
        XCTAssertEqual(try fm.destinationOfSymbolicLink(atPath: ancestor.path), root.path)
        if outside { XCTAssertEqual(try Data(contentsOf: selected), expected) }
    }
    private func hashes(_ urls: [URL]) throws -> [String] {
        try urls.map { url in SHA256.hash(data: try Data(contentsOf: url)).map { String(format: "%02x", $0) }.joined() }
    }

}
