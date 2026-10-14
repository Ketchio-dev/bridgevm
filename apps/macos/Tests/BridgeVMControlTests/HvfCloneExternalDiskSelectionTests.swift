import XCTest
@testable import BridgeVMControl

final class HvfCloneExternalDiskSelectionTests: XCTestCase {
    func testManagedDiskCopyMatchesAdmittedSelection() throws { try check(external: false) }
    func testExternalDiskCopyMustMatchAdmittedSelectionOrRefuseBeforeCopy() throws { try check(external: true) }

    private func check(external: Bool) throws {
        let fm = FileManager.default
        let root = URL(fileURLWithPath: ProcessInfo.processInfo.environment["TMPDIR"] ?? NSTemporaryDirectory())
            .appendingPathComponent("clone-selection-" + UUID().uuidString)
        let bundle = root.appendingPathComponent("source.bundle")
        let library = root.appendingPathComponent("library")
        let builtin = bundle.appendingPathComponent("disks/hvf-target.raw")
        let vars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
        let selected = root.appendingPathComponent("selected.raw")
        try fm.createDirectory(at: builtin.deletingLastPathComponent(), withIntermediateDirectories: true)
        try fm.createDirectory(at: vars.deletingLastPathComponent(), withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: root) }
        let builtinBytes = Data("bundled-unselected-disk".utf8)
        let selectedBytes = Data("registered-selected-disk".utf8)
        let varsBytes = Data("selected-vars".utf8)
        try builtinBytes.write(to: builtin)
        try selectedBytes.write(to: selected)
        try varsBytes.write(to: vars)
        let source = VMConfig(id: "source", name: "Source", displayName: "Source",
            backendKind: "hvf-engine", bootMode: "windows-hvf", bundlePath: bundle.path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
            sshUser: "", leasesPath: "", guestName: "source", displayWidth: 800, displayHeight: 600,
            diskPath: external ? selected.path : builtin.path)
        // This is an accepted legacy/saved config, not a fabricated unregistered operation.
        XCTAssertTrue(VMLibrary.migrateLegacyIfNeeded(rootURL: library, legacy: source))
        let loaded = try NativeLibraryReader.readConfig(rootURL: library, id: source.slug)
        XCTAssertEqual(loaded, source)
        XCTAssertTrue(VMLibrary.list(rootURL: library).contains(source))
        let engine = try XCTUnwrap(HvfEngineConfig.libraryVM(loaded, rootURL: library))
        XCTAssertEqual(engine.targetDiskPath, source.diskPath)
        XCTAssertEqual(HvfMediaLeaseSession.finalMediaPaths(source).disk, source.diskPath)
        let saved = try Data(contentsOf: library.appendingPathComponent("source/vm.json"))
        var inspected = false, enteredCopy = false
        let result = VMLibrary.cloneWindowsHVF(name: "Copy", template: loaded, libraryRoot: library,
            beforeCopy: { enteredCopy = true; return true }, afterCopy: {
                inspected = true
                let copy = library.appendingPathComponent("copy/bundle.vmbridge")
                XCTAssertEqual(try? Data(contentsOf: copy.appendingPathComponent("disks/hvf-target.raw")),
                    external ? selectedBytes : builtinBytes, "Copy must represent the disk selected by the admitted config")
                XCTAssertEqual(try? Data(contentsOf: copy.appendingPathComponent("metadata/hvf-vars.fd")), varsBytes)
                return false // Stop before vTPM identity/Keychain or clone registration.
            })
        XCTAssertEqual(enteredCopy, !external)
        XCTAssertEqual(inspected, !external)
        XCTAssertNil(result)
        XCTAssertFalse(fm.fileExists(atPath: library.appendingPathComponent("copy").path))
        XCTAssertEqual(try Data(contentsOf: builtin), builtinBytes)
        XCTAssertEqual(try Data(contentsOf: selected), selectedBytes)
        XCTAssertEqual(try Data(contentsOf: vars), varsBytes)
        XCTAssertEqual(try Data(contentsOf: library.appendingPathComponent("source/vm.json")), saved)
    }
}
