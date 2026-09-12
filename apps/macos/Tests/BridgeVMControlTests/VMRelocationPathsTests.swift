import XCTest
@testable import BridgeVMControl

final class VMRelocationPathsTests: XCTestCase {
    private func withRoot(_ body: (URL) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try body(root.resolvingSymlinksInPath())
    }

    func testResolvesExistingAliasBeforeMissingSuffix() throws {
        try withRoot { root in
            let source = root.appendingPathComponent("source")
            let alias = root.appendingPathComponent("alias")
            try FileManager.default.createDirectory(at: source, withIntermediateDirectories: false)
            try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: source)
            let destination = alias.appendingPathComponent("new/deep/bundle.vmbridge")
            XCTAssertEqual(VMRelocationPaths.prospectiveDirectory(destination)?.path,
                           source.appendingPathComponent("new/deep/bundle.vmbridge").path)
            XCTAssertFalse(VMRelocationPaths.isSafe(source: source, destination: destination))
            XCTAssertFalse(FileManager.default.fileExists(atPath: source.appendingPathComponent("new").path))
        }
    }

    func testDanglingAndCyclicAliasesAreNotAbsentDirectories() throws {
        try withRoot { root in
            let dangling = root.appendingPathComponent("dangling")
            let cyclic = root.appendingPathComponent("cyclic")
            try FileManager.default.createSymbolicLink(at: dangling, withDestinationURL: root.appendingPathComponent("absent"))
            try FileManager.default.createSymbolicLink(at: cyclic, withDestinationURL: cyclic)
            XCTAssertNil(VMRelocationPaths.prospectiveDirectory(dangling.appendingPathComponent("child")))
            XCTAssertNil(VMRelocationPaths.prospectiveDirectory(cyclic.appendingPathComponent("child")))
            XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("absent").path))
        }
    }

    func testRegularFileAncestorIsRejected() throws {
        try withRoot { root in
            let file = root.appendingPathComponent("file")
            try Data("not-directory".utf8).write(to: file)
            XCTAssertNil(VMRelocationPaths.prospectiveDirectory(file.appendingPathComponent("child")))
        }
    }

    func testSiblingPrefixRemainsAllowedAndRootIsRefused() throws {
        try withRoot { root in
            let source = root.appendingPathComponent("source")
            try FileManager.default.createDirectory(at: source, withIntermediateDirectories: false)
            XCTAssertTrue(VMRelocationPaths.isSafe(source: source, destination: root.appendingPathComponent("source-copy/bundle")))
            XCTAssertFalse(VMRelocationPaths.isSafe(source: source, destination: source))
            XCTAssertFalse(VMRelocationPaths.isSafe(source: URL(fileURLWithPath: "/"), destination: source))
        }
    }
}
