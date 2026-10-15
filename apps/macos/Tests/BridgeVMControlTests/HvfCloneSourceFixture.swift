import XCTest
@testable import BridgeVMControl

final class HvfCloneSourceFixture {
    let fm = FileManager.default
    let root = LibraryDeletionTestDirectory.temporary.appendingPathComponent("clone-source-" + UUID().uuidString)
    var bundle: URL { root.appendingPathComponent("source.bundle") }
    var disk: URL { bundle.appendingPathComponent("disks/hvf-target.raw") }
    var library: URL { root.appendingPathComponent("library") }
    let bytes = Data("owned selected disk".utf8)
    init() throws {
        try HvfCloneTestMedia.write(disk: disk, bytes: bytes)
    }
    deinit { try? fm.removeItem(at: root) }
    func config() -> VMConfig {
        VMConfig(id: "source", name: "Source", displayName: "Source", backendKind: "hvf-engine", bootMode: "windows-hvf",
            bundlePath: bundle.path, runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
            sshUser: "", leasesPath: "", guestName: "source", displayWidth: 800, displayHeight: 600)
    }
    func alias(_ name: String, to target: URL) throws -> URL {
        let link = root.appendingPathComponent(name)
        try fm.createSymbolicLink(at: link, withDestinationURL: target)
        return link
    }
    func checkCopy(_ config: VMConfig, allowed: Bool) throws {
        var before = false, after = false
        let result = VMLibrary.cloneWindowsHVF(name: "Copy", template: config, libraryRoot: library,
            beforeCopy: { before = true; return true }, afterCopy: {
                after = true
                let copied = self.library.appendingPathComponent("copy/bundle.vmbridge/disks/hvf-target.raw")
                XCTAssertEqual(try? Data(contentsOf: copied), self.bytes)
                XCTAssertEqual(try? copied.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink, false)
                return false // Never reach real key custody or registration, even on regression.
            })
        XCTAssertNil(result)
        XCTAssertEqual(before, allowed)
        XCTAssertEqual(after, allowed)
        XCTAssertFalse(fm.fileExists(atPath: library.appendingPathComponent("copy").path))
        if !allowed { XCTAssertFalse(fm.fileExists(atPath: library.path)) }
    }
}
