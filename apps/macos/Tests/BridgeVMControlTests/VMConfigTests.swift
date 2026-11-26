import XCTest
@testable import BridgeVMControl

final class VMConfigSlugifyTests: XCTestCase {
    func testBasicSlugify() {
        XCTAssertEqual(VMConfig.slugify("Ubuntu 2"), "ubuntu-2")
        XCTAssertEqual(VMConfig.slugify("Fedora 40"), "fedora-40")
    }

    func testCollapsesAndTrimsSeparators() {
        XCTAssertEqual(VMConfig.slugify("  --My  VM--  "), "my-vm")
        XCTAssertEqual(VMConfig.slugify("a..b__c"), "a-b-c")
        XCTAssertEqual(VMConfig.slugify("saved/vm"), "saved-vm")
        XCTAssertNotEqual(VMConfig.slugify("saved/vm"), VMConfig.slugify("savedvm"))
    }

    func testNeverEmpty() {
        // All-symbol or empty input must never yield an empty slug — an empty slug
        // would collapse the per-VM library path (~/…/vms/<slug>/) and clobber data.
        XCTAssertEqual(VMConfig.slugify("!!!"), "vm")
        XCTAssertEqual(VMConfig.slugify(""), "vm")
        XCTAssertFalse(VMConfig.slugify("@#$%").isEmpty)
    }

    func testPersistedIDCannotEscapeLibraryPath() {
        let cfg = VMConfig(id: "../../victim/../outside", name: "Safe VM", displayName: "Safe VM",
                           backendKind: "fast-vz", bootMode: nil, bundlePath: "", runnerPath: "",
                           launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                           leasesPath: "", guestName: "", displayWidth: 0, displayHeight: 0)
        XCTAssertEqual(cfg.slug, "victim-outside")
        XCTAssertFalse(cfg.slug.contains("/"))
        XCTAssertFalse(cfg.slug.contains(".."))
    }
}

final class BackendKindTests: XCTestCase {
    func testLabels() {
        XCTAssertEqual(BackendKind.hvfEngine.shortLabel, "HVF")
        XCTAssertEqual(BackendKind.hvfEngine.detailLabel, "Native (HVF)")
        XCTAssertEqual(BackendKind(savedKind: "hvf-engine"), .hvfEngine)
        XCTAssertEqual(BackendKind(savedKind: "bogus"), .unsupported)
    }

    /// A library saved before the Fast VZ and QEMU engines were removed must still
    /// load: those VMs are listed as unsupported, cannot start, and their files are
    /// never rewritten.
    func testSavedVMWithRemovedEngineIsReadAndMarkedUnsupported() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        var written: [String: Data] = [:]
        for (id, kind) in [("old-ubuntu", "fast-vz"), ("old-windows", "qemu-compat"), ("win", "hvf-engine")] {
            let object: [String: Any] = [
                "id": id, "name": id, "displayName": id, "backendKind": kind, "bootMode": "direct-kernel",
                "bundlePath": root.appendingPathComponent("\(id)/bundle.vmbridge").path,
                "runnerPath": "/missing/AppleVzRunner", "launchSpecPath": "", "handoffPath": "",
                "sshKeyPath": "", "sshUser": "user", "leasesPath": "", "guestName": id,
                "displayWidth": 1440, "displayHeight": 900, "memMiB": 4096, "cpuCount": 4,
            ]
            let directory = root.appendingPathComponent(id)
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            let data = try JSONSerialization.data(withJSONObject: object)
            try data.write(to: directory.appendingPathComponent("vm.json"))
            written[id] = data
        }

        let scan = VMLibrary.scan(rootURL: root)
        XCTAssertTrue(scan.issues.isEmpty)
        XCTAssertEqual(Set(scan.configs.map(\.slug)), ["old-ubuntu", "old-windows", "win"])
        for id in ["old-ubuntu", "old-windows"] {
            let config = try NativeLibraryReader.readConfig(rootURL: root, id: id)
            XCTAssertEqual(config.engineKind, .unsupported)
            let backend = config.makeBackend(libraryRoot: root)
            XCTAssertTrue(backend is UnsupportedEngineBackend)
            XCTAssertFalse(backend.start())
            XCTAssertFalse(backend.setResources(memMiB: 8192, cpu: 6))
            backend.stop()
            XCTAssertEqual(try Data(contentsOf: root.appendingPathComponent("\(id)/vm.json")), written[id])
        }
        XCTAssertEqual(try NativeLibraryReader.readConfig(rootURL: root, id: "win").engineKind, .hvfEngine)
    }

    func testMakeBackendReturnsHvfWindowsBackend() {
        let cfg = VMConfig(id: "win", name: "Windows", displayName: "Windows VM", backendKind: "hvf-engine",
                           bootMode: nil, bundlePath: "/tmp/win.vmbridge", runnerPath: "", launchSpecPath: "",
                           handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
                           guestName: "win", displayWidth: 1280, displayHeight: 800)
        XCTAssertTrue(cfg.makeBackend() is HvfWindowsBackend)
    }
}

final class VMLibraryPersistenceTests: XCTestCase {
    private func config(id: String = "saved-vm") -> VMConfig {
        VMConfig(id: id, name: "Saved VM", displayName: "Saved VM", backendKind: "fast-vz",
                 bootMode: nil, bundlePath: "", runnerPath: "", launchSpecPath: "",
                 handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
                 guestName: "", displayWidth: 0, displayHeight: 0)
    }

    func testSaveWritesNormalizedIdentityAtomically() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }

        XCTAssertTrue(VMLibrary.save(config(id: "../saved/vm"), rootURL: root))
        let url = root.appendingPathComponent("saved-vm/vm.json")
        let decoded = try JSONDecoder().decode(VMConfig.self, from: Data(contentsOf: url))
        XCTAssertEqual(decoded.id, "saved-vm")
    }

    func testSaveReportsUnwritableLibraryRoot() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try Data("not a directory".utf8).write(to: root)
        defer { try? FileManager.default.removeItem(at: root) }

        XCTAssertFalse(VMLibrary.save(config(), rootURL: root))
    }

    func testDeleteReportsSuccessAndRemovesOnlyNormalizedChild() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let vm = root.appendingPathComponent("safe-vm")
        try FileManager.default.createDirectory(at: vm, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        XCTAssertTrue(VMLibrary.delete("../safe/vm", rootURL: root))
        XCTAssertFalse(FileManager.default.fileExists(atPath: vm.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: root.path))
    }

    func testDeleteReportsMissingLibraryEntry() {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }

        XCTAssertFalse(VMLibrary.delete("missing", rootURL: root))
    }

    func testScanReturnsValidVMsAndReportsCorruptOrMissingConfigs() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        XCTAssertTrue(VMLibrary.save(config(id: "valid"), rootURL: root))
        let corrupt = root.appendingPathComponent("corrupt")
        let missing = root.appendingPathComponent("missing")
        try FileManager.default.createDirectory(at: corrupt, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: missing, withIntermediateDirectories: true)
        try Data("{not-json".utf8).write(to: corrupt.appendingPathComponent("vm.json"))

        let scan = VMLibrary.scan(rootURL: root)

        XCTAssertEqual(scan.configs.map(\.slug), ["valid"])
        XCTAssertEqual(scan.issues.count, 2)
        let issues = Dictionary(uniqueKeysWithValues: scan.issues.map { ($0.entry.lastPathComponent, $0) })
        let corruptIssue = try XCTUnwrap(issues["corrupt"])
        let missingIssue = try XCTUnwrap(issues["missing"])
        XCTAssertTrue(corruptIssue.message.contains("settings in vm.json"))
        XCTAssertTrue(missingIssue.message.contains("file exists and is readable"))
        XCTAssertTrue(corruptIssue.path.hasSuffix("/corrupt/vm.json"))
        XCTAssertTrue(missingIssue.path.hasSuffix("/missing/vm.json"))
    }

    func testScanQuarantinesNoncanonicalDirectoryWithoutConfusingCanonicalVM() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        var valid = config(id: "café-서울")
        valid.displayName = "Café 서울 — Windows"
        XCTAssertTrue(VMLibrary.save(valid, rootURL: root))
        let noncanonical = root.appendingPathComponent("café--서울", isDirectory: true)
        try FileManager.default.createDirectory(at: noncanonical, withIntermediateDirectories: true)
        let encoded = try JSONEncoder().encode(config(id: "café--서울"))
        try encoded.write(to: noncanonical.appendingPathComponent("vm.json"))

        let scan = VMLibrary.scan(rootURL: root)

        XCTAssertEqual(scan.configs, [valid])
        XCTAssertEqual(scan.issues.count, 1)
        XCTAssertEqual(
            URL(fileURLWithPath: scan.issues[0].path).resolvingSymlinksInPath(),
            noncanonical.resolvingSymlinksInPath()
        )
        XCTAssertTrue(scan.issues[0].message.contains("expected VM ID “café-서울”"))
        XCTAssertEqual(try Data(contentsOf: noncanonical.appendingPathComponent("vm.json")), encoded)
        XCTAssertTrue(FileManager.default.fileExists(atPath: root.appendingPathComponent("café-서울/vm.json").path))
    }

    func testScanQuarantinesUppercaseDirectoryInsteadOfRetargetingIt() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let noncanonical = root.appendingPathComponent("ManualVM", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: noncanonical, withIntermediateDirectories: true)
        try JSONEncoder().encode(config(id: "ManualVM"))
            .write(to: noncanonical.appendingPathComponent("vm.json"))

        let scan = VMLibrary.scan(rootURL: root)

        XCTAssertTrue(scan.configs.isEmpty)
        XCTAssertEqual(
            scan.issues.first.map { URL(fileURLWithPath: $0.path).resolvingSymlinksInPath() },
            noncanonical.resolvingSymlinksInPath()
        )
        XCTAssertTrue(scan.issues.first?.message.contains("manualvm") == true)
    }

    func testScanReportsLibraryRootThatIsAFile() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try Data("file".utf8).write(to: root)
        defer { try? FileManager.default.removeItem(at: root) }

        let scan = VMLibrary.scan(rootURL: root)

        XCTAssertTrue(scan.configs.isEmpty)
        XCTAssertEqual(scan.issues.count, 1)
        XCTAssertEqual(scan.issues.first?.path, root.path)
    }

    func testLegacyMigrationRefusesToRunWhenLibraryHasScanIssues() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let corrupt = root.appendingPathComponent("corrupt")
        try FileManager.default.createDirectory(at: corrupt, withIntermediateDirectories: true)
        try Data("broken".utf8).write(to: corrupt.appendingPathComponent("vm.json"))
        defer { try? FileManager.default.removeItem(at: root) }

        XCTAssertFalse(VMLibrary.migrateLegacyIfNeeded(rootURL: root, legacy: config(id: "legacy")))
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("legacy/vm.json").path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: corrupt.appendingPathComponent("vm.json").path))
    }

    func testScanAndSaveRejectSymlinkedVMDirectory() throws {
        let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let root = temp.appendingPathComponent("library")
        let outside = temp.appendingPathComponent("outside")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: outside, withIntermediateDirectories: true)
        XCTAssertTrue(VMLibrary.save(config(id: "linked"), rootURL: outside.deletingLastPathComponent()))
        // Move the valid config into the external target, then expose it only via a link.
        let generated = temp.appendingPathComponent("linked")
        if generated.path != outside.path {
            try? FileManager.default.removeItem(at: outside)
            try FileManager.default.moveItem(at: generated, to: outside)
        }
        try FileManager.default.createSymbolicLink(at: root.appendingPathComponent("linked"), withDestinationURL: outside)
        defer { try? FileManager.default.removeItem(at: temp) }

        let scan = VMLibrary.scan(rootURL: root)
        XCTAssertTrue(scan.configs.isEmpty)
        XCTAssertEqual(scan.issues.count, 1)
        XCTAssertTrue(scan.issues[0].message.contains("symbolic link"))
        XCTAssertFalse(VMLibrary.save(config(id: "linked"), rootURL: root))
    }

    func testScanAndSaveRejectSymlinkedConfigFileWithoutChangingTarget() throws {
        let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let root = temp.appendingPathComponent("library", isDirectory: true)
        let entry = root.appendingPathComponent("linked-config", isDirectory: true)
        let outside = temp.appendingPathComponent("outside.json")
        defer { try? FileManager.default.removeItem(at: temp) }
        try FileManager.default.createDirectory(at: entry, withIntermediateDirectories: true)
        let original = Data("external data must remain unchanged".utf8)
        try original.write(to: outside)
        try FileManager.default.createSymbolicLink(
            at: entry.appendingPathComponent("vm.json"),
            withDestinationURL: outside
        )

        let scan = VMLibrary.scan(rootURL: root)

        XCTAssertTrue(scan.configs.isEmpty)
        XCTAssertEqual(scan.issues.count, 1)
        XCTAssertTrue(scan.issues[0].message.contains("regular file"))
        XCTAssertFalse(VMLibrary.save(config(id: "linked-config"), rootURL: root))
        XCTAssertEqual(try Data(contentsOf: outside), original)
    }

    func testScanRejectsOversizedConfigBeforeDecoding() throws {
        let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let root = temp.appendingPathComponent("library", isDirectory: true)
        let entry = root.appendingPathComponent("oversized", isDirectory: true)
        let configURL = entry.appendingPathComponent("vm.json")
        defer { try? FileManager.default.removeItem(at: temp) }
        try FileManager.default.createDirectory(at: entry, withIntermediateDirectories: true)
        XCTAssertTrue(FileManager.default.createFile(atPath: configURL.path, contents: Data()))
        let handle = try FileHandle(forWritingTo: configURL)
        try handle.truncate(atOffset: UInt64(VMLibrary.maximumConfigBytes + 1))
        try handle.close()

        let scan = VMLibrary.scan(rootURL: root)

        XCTAssertTrue(scan.configs.isEmpty)
        XCTAssertEqual(scan.issues.count, 1)
        XCTAssertTrue(scan.issues[0].message.contains("\(VMLibrary.maximumConfigBytes) bytes"))
    }

    func testDeletionImpactDistinguishesManagedAndExternalBundles() {
        let root = URL(fileURLWithPath: "/tmp/bridgevm-library", isDirectory: true)
        var managed = config(id: "managed")
        managed.bundlePath = root.appendingPathComponent("managed/bundle.vmbridge").path
        var external = config(id: "external")
        external.bundlePath = "/Volumes/VMs/external/bundle.vmbridge"

        XCTAssertEqual(VMLibrary.deletionImpact(for: managed, rootURL: root), .managedBundleDeleted)
        XCTAssertEqual(VMLibrary.deletionImpact(for: external, rootURL: root), .registrationOnly)
    }

    func testDeletionImpactDoesNotTreatSiblingPrefixAsManaged() {
        let root = URL(fileURLWithPath: "/tmp/vms", isDirectory: true)
        var config = config(id: "win")
        config.bundlePath = "/tmp/vms/win-copy/bundle.vmbridge"

        XCTAssertEqual(VMLibrary.deletionImpact(for: config, rootURL: root), .registrationOnly)
    }

    func testDeletionImpactDoesNotClaimSymlinkTargetWillBeDeleted() throws {
        let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let root = temp.appendingPathComponent("library")
        let outside = temp.appendingPathComponent("outside")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: outside, withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(at: root.appendingPathComponent("linked"), withDestinationURL: outside)
        defer { try? FileManager.default.removeItem(at: temp) }
        var linked = config(id: "linked")
        linked.bundlePath = outside.appendingPathComponent("bundle.vmbridge").path

        XCTAssertEqual(VMLibrary.deletionImpact(for: linked, rootURL: root), .registrationOnly)
    }

    func testWindowsHVFMoveKeepsStableIdentityRebasesPathsAndWritesReceipt() throws {
        let temp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let root = temp.appendingPathComponent("library", isDirectory: true)
        let source = root.appendingPathComponent("win/bundle.vmbridge", isDirectory: true)
        let destinationParent = temp.appendingPathComponent("moved", isDirectory: true)
        try FileManager.default.createDirectory(
            at: source.appendingPathComponent("metadata/vtpm", isDirectory: true),
            withIntermediateDirectories: true
        )
        try Data("disk".utf8).write(to: source.appendingPathComponent("disks-placeholder"))
        defer { try? FileManager.default.removeItem(at: temp) }
        var cfg = config(id: "win")
        cfg.backendKind = "hvf-engine"
        cfg.bundlePath = source.path
        cfg.diskPath = source.appendingPathComponent("disks/hvf-target.raw").path
        XCTAssertTrue(VMLibrary.save(cfg, rootURL: root))

        let moved = try XCTUnwrap(VMLibrary.moveWindowsHVFBundle(
            cfg,
            to: destinationParent,
            rootURL: root
        ))

        XCTAssertEqual(moved.slug, "win")
        XCTAssertEqual(moved.bundlePath, destinationParent.appendingPathComponent("bundle.vmbridge").path)
        XCTAssertEqual(moved.diskPath, destinationParent.appendingPathComponent("bundle.vmbridge/disks/hvf-target.raw").path)
        XCTAssertFalse(FileManager.default.fileExists(atPath: source.path))
        let receipts = destinationParent.appendingPathComponent("bundle.vmbridge/metadata/vtpm-lifecycle")
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: receipts.path).count, 1)
        XCTAssertEqual(VMLibrary.list(rootURL: root).first?.id, "win")
        XCTAssertEqual(VMLibrary.list(rootURL: root).first?.bundlePath, moved.bundlePath)
    }
}

final class ShellCommandSafetyTests: XCTestCase {
    func testProcessLookupTreatsRegexMetacharactersAsLiteralData() throws {
        let suffix = UUID().uuidString
        let literalMarker = "bridgevm-[abc]-\(suffix)"
        let regexOnlyMatch = "bridgevm-a-\(suffix)"
        let decoy = try launchMarkerProcess(regexOnlyMatch)
        defer { terminate(decoy) }

        XCTAssertFalse(Shell.isProcessRunning(matching: literalMarker))

        let exact = try launchMarkerProcess(literalMarker)
        defer { terminate(exact) }
        XCTAssertTrue(Shell.isProcessRunning(matching: literalMarker))
    }

    func testShellCommandPreservesHostileArgumentsAsData() {
        let hostile = "quote' $(printf INJECTED) ; touch /tmp/never"
        let command = Shell.shellCommand("/usr/bin/printf", ["%s", hostile])
        let result = Shell.run("/bin/sh", ["-c", command])

        XCTAssertEqual(result.code, 0)
        XCTAssertEqual(result.output, hostile)
    }

    func testShellRunDrainsButRetainsOnlyBoundedOutputTail() {
        let result = Shell.run(
            "/usr/bin/printf",
            [String(repeating: "x", count: 10_000)],
            outputLimitBytes: 256
        )

        XCTAssertEqual(result.code, 0)
        XCTAssertTrue(result.output.hasPrefix("[출력 일부 생략"))
        XCTAssertTrue(result.output.hasSuffix(String(repeating: "x", count: 256)))
        XCTAssertLessThan(result.output.utf8.count, 400)
    }

    func testShellRunClosesPipeInheritedByGrandchild() {
        let started = Date()
        let result = Shell.run("/bin/sh", ["-c", "(/bin/sleep 2) & /usr/bin/printf done"], timeout: 1)

        XCTAssertEqual(result.code, 0)
        XCTAssertEqual(result.output, "done")
        XCTAssertLessThan(Date().timeIntervalSince(started), 1.5)
    }

    /// A command that finishes normally, but not instantly, must be waited for
    /// rather than killed. Nothing else covers this: the timeout test proves we
    /// kill when we should, and the fast commands elsewhere finish before any
    /// plausible wait expires, so a broken wait stays invisible there.
    func testShellRunWaitsForASlowCommandInsteadOfKillingIt() {
        let started = Date()
        let result = Shell.run("/bin/sh", ["-c", "/bin/sleep 0.6; /usr/bin/printf finished"], timeout: 20)

        XCTAssertEqual(result.output, "finished")
        XCTAssertEqual(result.code, 0, "a command inside its timeout must not be terminated")
        XCTAssertGreaterThan(Date().timeIntervalSince(started), 0.5)
    }

    /// The wait must also not add latency of its own: a command that exits
    /// immediately should return in well under the old 50 ms sampling period.
    func testShellRunReturnsPromptlyForAnImmediateCommand() {
        _ = Shell.run("/usr/bin/true", [])          // warm any first-call cost

        let started = Date()
        _ = Shell.run("/usr/bin/true", [])
        let elapsed = Date().timeIntervalSince(started)

        XCTAssertLessThan(elapsed, 0.05, "immediate command took \(elapsed)s; the wait is sampling, not blocking")
    }

    func testShellRunTimeoutTerminatesChildProcesses() throws {
        let pidFile = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-child-\(UUID().uuidString).pid")
        defer { try? FileManager.default.removeItem(at: pidFile) }
        let command = "/bin/sleep 30 & child=$!; echo $child > \(Shell.shQuote(pidFile.path)); wait $child"

        _ = Shell.run("/bin/sh", ["-c", command], timeout: 0.2)

        let childPID = try XCTUnwrap(
            pid_t(String(contentsOf: pidFile, encoding: .utf8).trimmingCharacters(in: .whitespacesAndNewlines))
        )
        let deadline = Date().addingTimeInterval(2)
        while kill(childPID, 0) == 0, Date() < deadline { usleep(20_000) }
        XCTAssertNotEqual(kill(childPID, 0), 0, "timed-out command left child process \(childPID) running")
    }

    private func launchMarkerProcess(_ marker: String) throws -> Process {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/sh")
        process.arguments = ["-c", "sleep 5; :", marker]
        try process.run()
        usleep(100_000)
        return process
    }

    private func terminate(_ process: Process) {
        if process.isRunning { process.terminate() }
        process.waitUntilExit()
    }
}

final class JSONFileSafetyTests: XCTestCase {
    func testRejectsSymlinkedJSONWithoutReadingOrChangingTarget() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let outside = temp.appendingPathComponent("outside.json")
        let linked = temp.appendingPathComponent("linked.json")
        let original = Data(#"{"safe":"external"}"#.utf8)
        try original.write(to: outside)
        try FileManager.default.createSymbolicLink(at: linked, withDestinationURL: outside)

        XCTAssertNil(JSONFile.loadDict(linked.path))
        XCTAssertFalse(JSONFile.writeDict(["changed": true], to: linked.path))
        XCTAssertEqual(try Data(contentsOf: outside), original)
    }

    func testRejectsJSONThroughSymlinkedMetadataDirectory() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let outside = temp.appendingPathComponent("outside", isDirectory: true)
        let metadata = temp.appendingPathComponent("metadata", isDirectory: true)
        try FileManager.default.createDirectory(at: outside, withIntermediateDirectories: true)
        try Data(#"{"safe":true}"#.utf8).write(to: outside.appendingPathComponent("launch.json"))
        try FileManager.default.createSymbolicLink(at: metadata, withDestinationURL: outside)
        let linkedFile = metadata.appendingPathComponent("launch.json")

        XCTAssertNil(JSONFile.loadDict(linkedFile.path))
        XCTAssertFalse(JSONFile.writeDict(["changed": true], to: linkedFile.path))
    }

    func testRejectsOversizedJSONBeforeLoading() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let file = temp.appendingPathComponent("large.json")
        XCTAssertTrue(FileManager.default.createFile(atPath: file.path, contents: Data()))
        let handle = try FileHandle(forWritingTo: file)
        try handle.truncate(atOffset: UInt64(JSONFile.maximumBytes + 1))
        try handle.close()

        XCTAssertNil(JSONFile.loadDict(file.path))
    }

    func testLoadsAndAtomicallyRewritesNormalJSON() throws {
        let temp = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: temp) }
        let file = temp.appendingPathComponent("normal.json")

        XCTAssertTrue(JSONFile.writeDict(["value": 1], to: file.path))
        XCTAssertEqual(JSONFile.loadDict(file.path)?["value"] as? Int, 1)
        XCTAssertTrue(JSONFile.writeDict(["value": 2], to: file.path))
        XCTAssertEqual(JSONFile.loadDict(file.path)?["value"] as? Int, 2)
    }

    private func makeTempDir() throws -> URL {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory
    }
}

@MainActor
final class ControlModelLogBoundTests: XCTestCase {
    func testBoundedLogKeepsNewestContentAndMarksOmission() {
        let value = "old-" + String(repeating: "n", count: 100)
        let bounded = ControlModel.boundedLog(value, limit: 12)

        XCTAssertTrue(bounded.hasPrefix("… 이전 로그 생략 …\n"))
        XCTAssertTrue(bounded.hasSuffix(String(repeating: "n", count: 12)))
        XCTAssertFalse(bounded.contains("old-"))
    }
}
