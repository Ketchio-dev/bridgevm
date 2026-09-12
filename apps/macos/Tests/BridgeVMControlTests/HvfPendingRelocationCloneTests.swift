import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfPendingRelocationCloneTests: XCTestCase {
    func testPendingRelocationRefusesCloneBeforeCopyingMedia() throws {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        addTeardownBlock { try? fm.removeItem(at: root) }
        let library = root.appendingPathComponent("library")
        let bundle = root.appendingPathComponent("source/bundle.vmbridge")
        try fm.createDirectory(at: bundle, withIntermediateDirectories: true)
        try Data("private-media".utf8).write(to: bundle.appendingPathComponent("disk.raw"))
        let config = VMConfig(id: "pending-source", name: "Pending Source",
            displayName: "Pending Source", backendKind: "hvf-engine",
            bundlePath: bundle.path, runnerPath: "", launchSpecPath: "",
            handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
            guestName: "Windows", displayWidth: 800, displayHeight: 600)
        XCTAssertTrue(VMLibrary.save(config, rootURL: library))
        var moved = config
        moved.bundlePath = root.appendingPathComponent("moved/bundle.vmbridge").path
        _ = try VMRelocationJournal.begin(original: config, moved: moved, rootURL: library)
        var copied = false
        let result = VMLibrary.cloneWindowsHVF(name: "Pending Copy", template: config,
                                              libraryRoot: library, afterCopy: {
            copied = true
            return false
        })
        XCTAssertNil(result)
        XCTAssertFalse(copied, "Unresolved relocation must be refused before copying media")
        XCTAssertTrue(VMRelocationJournal.isPending(config, rootURL: library))
        XCTAssertEqual(try Data(contentsOf: bundle.appendingPathComponent("disk.raw")), Data("private-media".utf8))
    }
}
