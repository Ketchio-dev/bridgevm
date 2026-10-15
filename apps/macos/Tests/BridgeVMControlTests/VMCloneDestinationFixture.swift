import XCTest
@testable import BridgeVMControl

final class VMCloneDestinationFixture {
    let fm = FileManager.default
    let root = LibraryDeletionTestDirectory.temporary.appendingPathComponent(UUID().uuidString)
    var source: URL { root.appendingPathComponent("source") }
    var library: URL { root.appendingPathComponent("library") }
    var sentinel: URL { source.appendingPathComponent("sentinel") }
    init() throws {
        try HvfCloneTestMedia.write(disk: source.appendingPathComponent("disks/hvf-target.raw"), bytes: Data("owned disk".utf8))
        try Data("keep source unchanged".utf8).write(to: sentinel)
    }
    deinit { try? fm.removeItem(at: root) }
    func config(windows: Bool) -> VMConfig {
        VMConfig(id: "source", name: "Source", displayName: "Source",
            backendKind: windows ? "hvf-engine" : "fast-vz", bundlePath: source.path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
            sshUser: "", leasesPath: "", guestName: "source", displayWidth: 800, displayHeight: 600)
    }
    func alias(_ name: String, to destination: URL) throws -> URL {
        let link = root.appendingPathComponent(name)
        try fm.createSymbolicLink(at: link, withDestinationURL: destination)
        return link
    }
}
