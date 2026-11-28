import XCTest
@testable import BridgeVMControl

struct FirstRunImportTransactionFixture: Sendable {
    let root = LibraryDeletionTestDirectory.temporary
        .appendingPathComponent("first-run-transaction-" + UUID().uuidString)
    var library: URL { root.appendingPathComponent("library") }
    var entry: URL { library.appendingPathComponent("imported") }
    var disk: URL { root.appendingPathComponent("disk.raw") }
    var vars: URL { root.appendingPathComponent("vars.fd") }

    init() throws {
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        try Data([1, 2, 3, 4]).write(to: disk)
        try Data([5, 6, 7, 8]).write(to: vars)
        let handle = try FileHandle(forWritingTo: vars)
        try handle.truncate(atOffset: FirstRunImport.requiredVarsBytes)
        try handle.close()
    }

    func inputs(_ name: String = "imported") -> FirstRunImport.Inputs {
        .init(displayName: name, diskPath: disk.path, varsPath: vars.path,
              vtpmStateDir: nil, memMiB: 4096, cpuCount: 2)
    }

    func assertPersisted(file: StaticString = #filePath, line: UInt = #line) throws {
        let configs = VMLibrary.list(rootURL: library)
        XCTAssertEqual(configs.map(\.slug), ["imported"], file: file, line: line)
        let config = try XCTUnwrap(configs.first, file: file, line: line)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: XCTUnwrap(config.diskPath))),
            Data([1, 2, 3, 4]), file: file, line: line)
        let layout = FirstRunImport.BundleLayout(bundleURL: URL(fileURLWithPath: config.bundlePath))
        XCTAssertEqual(try Data(contentsOf: layout.varsURL), try Data(contentsOf: vars), file: file, line: line)
        try assertSources(file: file, line: line)
    }

    func assertSources(file: StaticString = #filePath, line: UInt = #line) throws {
        XCTAssertEqual(try Data(contentsOf: disk), Data([1, 2, 3, 4]), file: file, line: line)
        var expected = Data([5, 6, 7, 8])
        expected.count = Int(FirstRunImport.requiredVarsBytes)
        XCTAssertEqual(try Data(contentsOf: vars), expected, file: file, line: line)
    }

    func remove() { try? FileManager.default.removeItem(at: root) }
}

final class FirstRunImportTransactionTests: XCTestCase {
    func testMissingImportToolOffersAppRepairInBothForms() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let tool = fixture.root.appendingPathComponent("missing-tool")
        let guidance = "The disk import tool could not start. Reinstall BridgeVM or rebuild the app if you built it from source."

        let firstRun = FirstRunImportWorker.run(fixture.inputs(), libraryRoot: fixture.library, snapshotHelper: tool)
        if case .failed(let message) = firstRun { XCTAssertEqual(message, guidance) }
        else { XCTFail("The first-run form must explain the missing tool") }

        let create = VMLibrary.createWindowsHVF(name: "imported", targetDiskPath: fixture.disk.path,
            varsPath: fixture.vars.path, libraryRoot: fixture.library, snapshotHelper: tool)
        if case .refused(let message) = create { XCTAssertEqual(message, guidance) }
        else { XCTFail("The Create form must explain the missing tool") }

        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path))
        XCTAssertEqual(try Data(contentsOf: fixture.disk), Data([1, 2, 3, 4]))
    }

    func testConfigWriteRefusalRemovesPreparedBundle() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let result = FirstRunImportWorker.run(fixture.inputs(), libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper, publish: { config, root in
            VMLibrary.createOutcome(config, rootURL: root) { _, _ in .notPublished(CocoaError(.fileWriteOutOfSpace)) }
        })
        guard case .failed = result else { return XCTFail("refused config write must fail") }
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path))
        try fixture.assertSources()
    }

    /// A running VM holds its runtime writer lease on the disk, as on a BridgeVM VM's hvf-target.raw.
    func testDiskInUseByARunningVMSaysToShutItDown() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let lock = open(fixture.disk.path + ".bridgevm-writer.lock", O_RDWR | O_CREAT, 0o600)
        XCTAssertGreaterThanOrEqual(lock, 0)
        defer { close(lock) }
        XCTAssertEqual(flock(lock, LOCK_EX | LOCK_NB), 0)
        let result = FirstRunImportWorker.run(fixture.inputs(), libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper)
        guard case .failed(let message) = result else { return XCTFail("in-use disk must refuse: \(result)") }
        XCTAssertEqual(message, "Shut down the VM that uses this disk, then import again.")
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path))
        try fixture.assertSources()
    }

    /// Another VM app running the VM holds its disk open; only BridgeVM's own lock was checked, so the import
    /// copied a disk that was still being written.
    func testDiskAnotherAppHasOpenIsRefusedByBothImportForms() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let own = try FileHandle(forReadingFrom: fixture.disk)
        defer { try? own.close() }
        XCTAssertFalse(HvfMediaSourceHolder.othersHaveOpen(fixture.disk.path), "this app's own reads don't count")
        let holder = Process()
        holder.executableURL = URL(fileURLWithPath: "/bin/sh")
        holder.arguments = ["-c", "exec 3<>\"$0\"; exec sleep 30", fixture.disk.path]
        try holder.run()
        defer { holder.terminate(); holder.waitUntilExit() }
        let deadline = Date().addingTimeInterval(5)
        while !HvfMediaSourceHolder.othersHaveOpen(fixture.disk.path), Date() < deadline { usleep(20_000) }
        let result = FirstRunImportWorker.run(fixture.inputs(), libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper)
        guard case .failed(let message) = result else { return XCTFail("a disk another app has open must refuse: \(result)") }
        XCTAssertEqual(message, "Shut down the VM that uses this disk, then import again.")
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path))
        guard case .refused(let sheet)? = VMLibrary.createWindowsHVF(name: "held", targetDiskPath: fixture.disk.path,
            varsPath: fixture.vars.path, libraryRoot: fixture.library, persist: false,
            snapshotHelper: HvfMediaImportTestSupport.helper) else { return XCTFail("the Create sheet must say why") }
        XCTAssertEqual(sheet, message)
        try fixture.assertSources()
    }

    /// A RAW disk whose copy failed (its drive unplugged mid-copy) got the Create sheet's generic
    /// "Couldn’t create the VM or save it to the library." instead of what to check.
    func testAFailedRawCopySaysWhatToCheck() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        guard case .refused(let sentence)? = VMLibrary.createWindowsHVF(name: "copy fails", targetDiskPath: fixture.disk.path,
            varsPath: fixture.vars.path, libraryRoot: fixture.library, persist: false,
            snapshotHelper: URL(fileURLWithPath: "/usr/bin/false")) else { return XCTFail("the Create sheet must say why") }
        XCTAssertEqual(sentence, WindowsFileFailureFeedback.importing)
        try fixture.assertSources()
    }

    /// Too little room on the library's drive was found only when the copy filled it to the last byte
    /// (pausing VMs running from it), after as long as the copy took.
    func testTooLittleFreeSpaceIsSaidBeforeCopying() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let (image, _) = HvfQCOW2TestImage.make()
        try image.write(to: fixture.disk)
        let none: (URL) -> Int64? = { _ in 0 }
        XCTAssertTrue(HvfImportSpace.tooLittle(disk: fixture.disk, into: fixture.root, available: none),
                      "a converted disk needs room, even on its own drive")
        XCTAssertFalse(HvfImportSpace.tooLittle(disk: fixture.disk, into: fixture.root, available: { _ in 1 << 40 }))
        XCTAssertThrowsError(try HvfMediaImport.copy(disk: fixture.disk.path, vars: "",
            toDisk: fixture.root.appendingPathComponent("copy/disk.raw"),
            toVars: fixture.root.appendingPathComponent("copy/vars.fd"),
            helper: HvfMediaImportTestSupport.helper, available: none)) { error in
            XCTAssertEqual(WindowsFileFailureFeedback.importRemedy(for: error),
                "There isn’t enough free space to copy the disk. Free up space on the drive that holds your VM library, then import again.")
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.root.appendingPathComponent("copy").path))
        try Data(repeating: 7, count: 4096).write(to: fixture.disk)
        XCTAssertFalse(HvfImportSpace.tooLittle(disk: fixture.disk, into: fixture.root, available: none),
                       "a RAW disk on its own drive is cloned and needs no room")
    }

    func testCommittedImportPersistsIndependentPairAndConfig() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let result = FirstRunImportWorker.run(fixture.inputs(), libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper)
        guard case .committed = result else { return XCTFail("import must commit") }
        try fixture.assertPersisted()
    }

    func testAmbiguousRegistrationRefusalPreservesPreparedEntry() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let marker = fixture.root.appendingPathComponent("unrelated")
        try Data([9]).write(to: marker)
        var attemptedCommit = false
        let result = FirstRunImportWorker.run(fixture.inputs(), libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper, publish: { config, root in
            do {
                try FileManager.default.createDirectory(at: fixture.entry.appendingPathComponent("vm.json"),
                    withIntermediateDirectories: false)
            } catch { return .notPublished(error) }
            attemptedCommit = true
            return VMLibrary.createOutcome(config, rootURL: root)
        })
        guard case .failed = result else { return XCTFail("rename onto directory must refuse") }
        XCTAssertTrue(attemptedCommit)
        try fixture.assertAmbiguousPreparationPreserved()
        XCTAssertEqual(try Data(contentsOf: marker), Data([9]))
        try fixture.assertSources()
    }

    func testPublishedConfigAndMediaSurviveDirectorySyncFailure() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let result = FirstRunImportWorker.run(fixture.inputs(), libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper, publish: { config, root in
            VMLibrary.createOutcome(config, rootURL: root) { data, url in
                VMRegistrationWriter.commit(data, to: url, replaceExisting: false, syncParent: { _ in throw CocoaError(.fileWriteUnknown) })
            }
        })
        guard case .publishedButUnsynced = result else { return XCTFail("publication must be retained") }
        try fixture.assertPersisted()
    }

    func testFailureDoesNotDeleteReplacementOfReservedEntry() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let moved = fixture.root.appendingPathComponent("moved-preparation")
        let marker = fixture.entry.appendingPathComponent("replacement")
        let result = FirstRunImportWorker.run(fixture.inputs(), libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper, publish: { _, _ in
            do {
                try FileManager.default.moveItem(at: fixture.entry, to: moved)
                try FileManager.default.createDirectory(at: fixture.entry, withIntermediateDirectories: false)
                try Data([9]).write(to: marker)
            } catch { return .notPublished(error) }
            return .notPublished(CocoaError(.fileWriteUnknown))
        })
        guard case .failed = result else { return XCTFail("injected refusal must fail") }
        XCTAssertEqual(try Data(contentsOf: marker), Data([9]))
        XCTAssertEqual(try Data(contentsOf: moved.appendingPathComponent("bundle/disks/hvf-target.raw")),
            Data([1, 2, 3, 4]))
        try fixture.assertSources()
    }
}
