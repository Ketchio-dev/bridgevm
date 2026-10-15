import Darwin
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfCloneVarsFixture {
    enum Layout { case regular, varsLink, metadataLink }
    let root: URL
    let config: VMConfig
    let target: URL
    let sourceVars: URL
    let libraryRoot: URL
    let bytes = Data("owned independent vars".utf8)
    let fm = FileManager.default

    init(_ layout: Layout) throws {
        root = try Self.directory()
        config = VMConfig(id: "source", name: "source", displayName: "source", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: root.appendingPathComponent("source/bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: "source", displayWidth: 1280, displayHeight: 800)
        let bundle = URL(fileURLWithPath: config.bundlePath)
        let disk = bundle.appendingPathComponent("disks/hvf-target.raw")
        let metadata = bundle.appendingPathComponent("metadata")
        sourceVars = metadata.appendingPathComponent("hvf-vars.fd")
        libraryRoot = root.appendingPathComponent("library")
        try fm.createDirectory(at: disk.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("owned independent disk".utf8).write(to: disk)
        switch layout {
        case .regular:
            try fm.createDirectory(at: metadata, withIntermediateDirectories: false)
            target = sourceVars
        case .varsLink:
            try fm.createDirectory(at: metadata, withIntermediateDirectories: false)
            target = root.appendingPathComponent("external-vars.fd")
            try fm.createSymbolicLink(at: sourceVars, withDestinationURL: target)
        case .metadataLink:
            let external = root.appendingPathComponent("external-metadata")
            try fm.createDirectory(at: external, withIntermediateDirectories: false)
            try fm.createSymbolicLink(at: metadata, withDestinationURL: external)
            target = external.appendingPathComponent("hvf-vars.fd")
        }
        try bytes.write(to: target)
        XCTAssertTrue(VMLibrary.migrateLegacyIfNeeded(rootURL: libraryRoot, legacy: config))
    }

    func checkCopy(allowed: Bool) throws {
        var before = false, after = false
        let identity = try inode(target)
        let result = VMLibrary.cloneWindowsHVF(name: "Copy", template: config, libraryRoot: libraryRoot,
            beforeCopy: { before = true; return true }, afterCopy: {
                after = true
                let copied = self.libraryRoot.appendingPathComponent("copy/bundle.vmbridge/metadata/hvf-vars.fd")
                let copiedIdentity = try? self.inode(copied)
                print("clone-vars-copy allowed=\(allowed) shared=\(copiedIdentity == identity) source=\(identity) copy=\(copiedIdentity ?? "unavailable")")
                XCTAssertEqual(try? Data(contentsOf: copied), self.bytes)
                XCTAssertNotEqual(copiedIdentity, identity, "Copied vars must have independent file identity")
                return false // Stop before all Keychain and registration work.
            })
        XCTAssertNil(result)
        XCTAssertEqual(before, allowed)
        XCTAssertEqual(after, allowed)
        XCTAssertEqual(try inode(target), identity)
        XCTAssertEqual(try Data(contentsOf: target), bytes)
        XCTAssertFalse(fm.fileExists(atPath: libraryRoot.appendingPathComponent("copy").path))
    }

    func inode(_ path: URL) throws -> String {
        var info = stat()
        guard stat(path.path, &info) == 0 else { throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
        return "\(info.st_dev):\(info.st_ino)"
    }

    func clean() { try? fm.removeItem(at: root) }

    private static func directory() throws -> URL {
        let root = LibraryDeletionTestDirectory.temporary.appendingPathComponent("clone-vars-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false,
            attributes: [.posixPermissions: 0o700])
        return root // Cleanup ownership exists only after exclusive creation succeeds.
    }
}
