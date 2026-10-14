import XCTest
@testable import BridgeVMControl

final class FirstRunImportAliasAdmissionTests: XCTestCase {
    func testWorkerImportsRegularVarsAndReadsBackRegistration() throws {
        try assertImport(usingVarsAlias: false)
    }

    func testWorkerImportsSymlinkedVarsAndReadsBackRegistration() throws {
        try assertImport(usingVarsAlias: true)
    }

    func testWorkerImportsSymlinkedDiskAndVarsAndReadsBackRegistration() throws {
        try assertImport(usingDiskAlias: true, usingVarsAlias: true)
    }

    func testWorkerRefusesDirectEmptyDisk() throws {
        try assertEmptyDiskRefused(usingAlias: false)
    }

    func testWorkerRefusesSymlinkedEmptyDisk() throws {
        try assertEmptyDiskRefused(usingAlias: true)
    }

    func testWorkerRefusesWrongSizedVarsAliasWithTargetSize() throws {
        try assertVarsAliasRefused(.wrongSize)
    }

    func testWorkerRefusesDanglingVarsAlias() throws {
        try assertVarsAliasRefused(.dangling)
    }

    func testWorkerRefusesDirectoryVarsAlias() throws {
        try assertVarsAliasRefused(.directory)
    }

    private struct Identity: Equatable {
        let device: UInt64
        let inode: UInt64
        let bytes: UInt64

        init(_ url: URL) throws {
            let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
            device = try XCTUnwrap(attributes[.systemNumber] as? NSNumber).uint64Value
            inode = try XCTUnwrap(attributes[.systemFileNumber] as? NSNumber).uint64Value
            bytes = try XCTUnwrap(attributes[.size] as? NSNumber).uint64Value
        }
    }

    private struct Fingerprint: Equatable {
        let identity: Identity
        let hash: String

        init(_ url: URL) throws {
            identity = try Identity(url)
            hash = try XCTUnwrap(HvfWindowsInstallCacheIdentity.sha256File(url.path))
        }
    }

    private func assertEmptyDiskRefused(usingAlias: Bool) throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        try Data().write(to: fixture.disk)
        let alias = fixture.root.appendingPathComponent("selected-disk.raw")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: fixture.disk)
        let aliasBefore = try Identity(alias)
        let diskBefore = try Fingerprint(fixture.disk)
        let varsBefore = try Fingerprint(fixture.vars)
        var inputs = fixture.inputs()
        if usingAlias { inputs.diskPath = alias.path }
        let expected = FirstRunImport.ValidationError.diskEmpty(inputs.diskPath)
        XCTAssertEqual(FirstRunImport.validate(inputs), expected)
        let result = FirstRunImportWorker.run(inputs, libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper)
        XCTAssertEqual(try Fingerprint(fixture.disk), diskBefore)
        XCTAssertEqual(try Fingerprint(fixture.vars), varsBefore)
        XCTAssertEqual(try Identity(alias), aliasBefore)
        XCTAssertEqual(try FileManager.default.destinationOfSymbolicLink(atPath: alias.path), fixture.disk.path)
        if case .committed(let config) = result {
            XCTAssertEqual(VMLibrary.list(rootURL: fixture.library), [config])
            let copiedDisk = URL(fileURLWithPath: try XCTUnwrap(config.diskPath))
            XCTAssertEqual(try Identity(copiedDisk).bytes, 0)
            return XCTFail("Empty disk was committed and read back with a zero-byte copied disk")
        }
        guard case .failed(let message) = result else { return XCTFail("Unexpected result: \(result)") }
        XCTAssertEqual(message, expected.description)
        XCTAssertTrue(VMLibrary.list(rootURL: fixture.library).isEmpty)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path))
    }

    private enum InvalidVarsTarget { case wrongSize, dangling, directory }

    private func assertVarsAliasRefused(_ kind: InvalidVarsTarget) throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let target = fixture.root.appendingPathComponent("invalid-vars")
        let alias = fixture.root.appendingPathComponent("selected-vars.fd")
        let marker = target.appendingPathComponent("keep")
        var targetBefore: Identity?
        var targetHash: String?
        let expected: FirstRunImport.ValidationError
        switch kind {
        case .wrongSize:
            try Data([9, 8, 7, 6]).write(to: target)
            targetHash = try Fingerprint(target).hash
            expected = .varsWrongSize(path: alias.path, bytes: 4)
        case .dangling:
            expected = .varsMissing(alias.path)
        case .directory:
            try FileManager.default.createDirectory(at: target, withIntermediateDirectories: false)
            try Data([9, 8, 7, 6]).write(to: marker)
            expected = .varsMissing(alias.path)
        }
        if kind != .dangling { targetBefore = try Identity(target) }
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: target)
        let aliasBefore = try Identity(alias)
        let diskBefore = try Fingerprint(fixture.disk)
        let varsBefore = try Fingerprint(fixture.vars)
        var inputs = fixture.inputs()
        inputs.varsPath = alias.path
        XCTAssertEqual(FirstRunImport.validate(inputs), expected)
        let result = FirstRunImportWorker.run(inputs, libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper)
        guard case .failed(let message) = result else { return XCTFail("Invalid alias must refuse: \(result)") }
        XCTAssertEqual(message, expected.description)
        XCTAssertTrue(VMLibrary.list(rootURL: fixture.library).isEmpty)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path))
        XCTAssertEqual(try Fingerprint(fixture.disk), diskBefore)
        XCTAssertEqual(try Fingerprint(fixture.vars), varsBefore)
        XCTAssertEqual(try Identity(alias), aliasBefore)
        XCTAssertEqual(try FileManager.default.destinationOfSymbolicLink(atPath: alias.path), target.path)
        if let targetBefore { XCTAssertEqual(try Identity(target), targetBefore) }
        if let targetHash { XCTAssertEqual(try Fingerprint(target).hash, targetHash) }
        if kind == .dangling { XCTAssertFalse(FileManager.default.fileExists(atPath: target.path)) }
        if kind == .directory {
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: target.path), ["keep"])
            XCTAssertEqual(try Data(contentsOf: marker), Data([9, 8, 7, 6]))
        }
    }

    private func assertImport(usingDiskAlias: Bool = false, usingVarsAlias: Bool) throws {
        // This fixture uses the explicit TMPDIR helper, not Foundation's cached
        // temporary directory. Its vars ftruncate is exactly 64 MiB, sparse.
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let alias = fixture.root.appendingPathComponent("selected-vars.fd")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: fixture.vars)
        let diskAlias = fixture.root.appendingPathComponent("selected-disk.raw")
        try FileManager.default.createSymbolicLink(at: diskAlias, withDestinationURL: fixture.disk)
        let aliasBefore = try Identity(alias)
        let diskAliasBefore = try Identity(diskAlias)
        let diskBefore = try Fingerprint(fixture.disk)
        let varsBefore = try Fingerprint(fixture.vars)
        var inputs = fixture.inputs()
        if usingDiskAlias { inputs.diskPath = diskAlias.path }
        if usingVarsAlias { inputs.varsPath = alias.path }

        XCTAssertNil(FirstRunImport.validate(inputs), "A valid vars reference must pass worker admission")
        let result = FirstRunImportWorker.run(inputs, libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper)

        XCTAssertEqual(try Fingerprint(fixture.disk), diskBefore)
        XCTAssertEqual(try Fingerprint(fixture.vars), varsBefore)
        XCTAssertEqual(try Identity(alias), aliasBefore)
        XCTAssertEqual(try Identity(diskAlias), diskAliasBefore)
        XCTAssertEqual(try FileManager.default.destinationOfSymbolicLink(atPath: alias.path), fixture.vars.path)
        XCTAssertEqual(try FileManager.default.destinationOfSymbolicLink(atPath: diskAlias.path), fixture.disk.path)
        guard case .committed(let config) = result else {
            return XCTFail("Valid first-run import must commit: \(result)")
        }
        XCTAssertEqual(VMLibrary.list(rootURL: fixture.library), [config])
        let layout = FirstRunImport.BundleLayout(bundleURL: URL(fileURLWithPath: config.bundlePath))
        let copiedDisk = try Fingerprint(layout.diskURL)
        let copiedVars = try Fingerprint(layout.varsURL)
        XCTAssertEqual(copiedDisk.hash, diskBefore.hash)
        XCTAssertEqual(copiedVars.hash, varsBefore.hash)
        XCTAssertEqual(copiedVars.identity.bytes, FirstRunImport.requiredVarsBytes)
        XCTAssertNotEqual(copiedDisk.identity.inode, diskBefore.identity.inode)
        XCTAssertNotEqual(copiedVars.identity.inode, varsBefore.identity.inode)

        let writer = try FileHandle(forWritingTo: layout.varsURL)
        try writer.write(contentsOf: Data("independent guest vars".utf8))
        try writer.close()
        XCTAssertEqual(try Fingerprint(fixture.disk), diskBefore)
        XCTAssertEqual(try Fingerprint(fixture.vars), varsBefore)
        XCTAssertEqual(try Identity(alias), aliasBefore)
        XCTAssertEqual(try Identity(diskAlias), diskAliasBefore)
    }
}
