import XCTest
@testable import BridgeVMControl

final class VMRelocationObservedStateTests: XCTestCase {
    private final class AfterEffect: FileManager, @unchecked Sendable {
        var calls = 0
        override func moveItem(at source: URL, to destination: URL) throws {
            calls += 1
            try super.moveItem(at: source, to: destination)
            if calls == 2 { throw CocoaError(.fileWriteUnknown) }
        }
    }
    private final class Unreadable: FileManager, @unchecked Sendable {
        override func attributesOfItem(atPath path: String) throws -> [FileAttributeKey: Any] {
            throw CocoaError(.fileReadNoPermission)
        }
    }
    private func root() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        return root
    }
    private func config(_ bundle: URL) -> VMConfig {
        VMConfig(id: "observed-move", name: "Move", displayName: "Move",
            backendKind: "hvf-engine", bootMode: nil, bundlePath: bundle.path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
            sshUser: "", leasesPath: "", guestName: "observed-move", displayWidth: 800, displayHeight: 600)
    }

    func testRollbackErrorAfterRenameRegistersActualOriginalLocation() throws {
        let root = try root()
        let library = root.appendingPathComponent("library")
        let source = root.appendingPathComponent("source/bundle.vmbridge")
        let parent = root.appendingPathComponent("destination")
        try FileManager.default.createDirectory(at: source.appendingPathComponent("metadata/vtpm"), withIntermediateDirectories: true)
        try Data("receipt-blocker".utf8).write(to: source.appendingPathComponent("metadata/vtpm-lifecycle"))
        try Data("retained-media".utf8).write(to: source.appendingPathComponent("disk"))
        let original = config(source)
        XCTAssertTrue(VMLibrary.save(original, rootURL: library))
        let manager = AfterEffect()
        XCTAssertNil(VMLibrary.moveWindowsHVFBundle(original, to: parent, rootURL: library, fileManager: manager))
        XCTAssertEqual(manager.calls, 2)
        XCTAssertFalse(FileManager.default.fileExists(atPath: parent.appendingPathComponent("bundle.vmbridge").path))
        XCTAssertEqual(VMLibrary.list(rootURL: library).first?.bundlePath, source.path)
        XCTAssertEqual(try Data(contentsOf: source.appendingPathComponent("disk")), Data("retained-media".utf8))
    }

    func testUniqueDirectorySelectsItsLocationAndAmbiguityDoesNotGuess() throws {
        let root = try root()
        let source = root.appendingPathComponent("source")
        let target = root.appendingPathComponent("target")
        let original = config(source), moved = config(target)
        XCTAssertNil(VMRelocationRecovery.configuration(original: original, moved: moved))
        try FileManager.default.createDirectory(at: source, withIntermediateDirectories: false)
        XCTAssertEqual(VMRelocationRecovery.configuration(original: original, moved: moved), original)
        try FileManager.default.createDirectory(at: target, withIntermediateDirectories: false)
        XCTAssertNil(VMRelocationRecovery.configuration(original: original, moved: moved))
        try FileManager.default.removeItem(at: source)
        XCTAssertEqual(VMRelocationRecovery.configuration(original: original, moved: moved), moved)
    }

    func testSymlinksAndUnreadableLocationsAreNotTreatedAsAbsent() throws {
        let root = try root()
        let source = root.appendingPathComponent("source")
        let alias = root.appendingPathComponent("alias")
        try FileManager.default.createDirectory(at: source, withIntermediateDirectories: false)
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: source)
        XCTAssertNil(VMRelocationRecovery.configuration(original: config(source), moved: config(alias)))
        XCTAssertNil(VMRelocationRecovery.configuration(original: config(source),
            moved: config(root.appendingPathComponent("absent")), fileManager: Unreadable()))
    }
}
