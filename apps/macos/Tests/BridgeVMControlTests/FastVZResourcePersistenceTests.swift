import XCTest
@testable import BridgeVMControl

/// Advanced > Resources > Apply must leave the launch spec (VZ configuration),
/// the handoff (runner validation) and vm.json (library card, workspace list,
/// native CLI inspect) agreeing on one allocation, or change none of them.
final class FastVZResourcePersistenceTests: XCTestCase {
    private struct Fixture {
        let temp = URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
            .appendingPathComponent("fastvz-resources-" + UUID().uuidString, isDirectory: true)
        var library: URL { temp.appendingPathComponent("library", isDirectory: true) }
        var metadata: URL { temp.appendingPathComponent("bundle.vmbridge/metadata", isDirectory: true) }
        var launchSpec: URL { metadata.appendingPathComponent("apple-vz-launch.json") }
        var handoff: URL { metadata.appendingPathComponent("handoff.json") }
        // Unique per run, so a backend that ignored the injected root could never
        // collide with a real VM in the global library.
        var slug: String { VMConfig.slugify(temp.lastPathComponent) }

        static let resources: [String: Any] = [
            "memory": "4096", "cpu": "2", "display_fps_cap": "60",
            "rationale": "fixture", "balloon_device": true,
        ]

        var config: VMConfig {
            VMConfig(id: slug, name: slug, displayName: "Fast Resources", backendKind: "fast-vz",
                     bootMode: "direct-kernel", bundlePath: metadata.deletingLastPathComponent().path,
                     runnerPath: "/nonexistent/AppleVzRunner", launchSpecPath: launchSpec.path,
                     handoffPath: handoff.path, sshKeyPath: "", sshUser: "user", leasesPath: "",
                     guestName: slug, displayWidth: 1440, displayHeight: 900,
                     installPending: false, memMiB: 4096, cpuCount: 2)
        }

        func prepare(launchResources: Bool = true, handoffResources: Bool = true) throws {
            try FileManager.default.createDirectory(at: metadata, withIntermediateDirectories: true)
            var launch: [String: Any] = ["kernel": "/bundle/vmlinuz", "cmdline": "console=hvc0", "disk": ["path": "/bundle/root.img"]]
            if launchResources { launch["resources"] = Self.resources }
            var handoffObject: [String: Any] = ["backend": "apple-virtualization-framework", "vm_name": slug,
                                                "launch_spec_path": launchSpec.path, "boot_mode": "linux-kernel"]
            if handoffResources { handoffObject["resources"] = Self.resources }
            try JSONSerialization.data(withJSONObject: launch, options: [.sortedKeys]).write(to: launchSpec)
            try JSONSerialization.data(withJSONObject: handoffObject, options: [.sortedKeys]).write(to: handoff)
        }

        func object(_ url: URL) throws -> NSDictionary {
            try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? NSDictionary)
        }

        func clean() {
            try? FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: metadata.path)
            try? FileManager.default.removeItem(at: temp)
        }
    }

    private func assertNoGlobalLibraryLeak(_ fixture: Fixture, file: StaticString = #filePath, line: UInt = #line) {
        let leaked = VMLibrary.root.appendingPathComponent(fixture.slug).path
        XCTAssertFalse(FileManager.default.fileExists(atPath: leaked),
                       "setResources wrote the global library instead of the injected root", file: file, line: line)
    }

    func testApplyPersistsLaunchSpecHandoffAndLibraryRegistration() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        try fixture.prepare()
        let config = fixture.config
        XCTAssertTrue(VMLibrary.save(config, rootURL: fixture.library))
        let launchBefore = try fixture.object(fixture.launchSpec)
        let handoffBefore = try fixture.object(fixture.handoff)
        let backend = config.makeBackend(libraryRoot: fixture.library)

        XCTAssertTrue(backend.setResources(memMiB: 8192, cpu: 6))

        assertNoGlobalLibraryLeak(fixture)
        let current = backend.resources()
        XCTAssertEqual(current.memMiB, 8192)
        XCTAssertEqual(current.cpu, 6)

        let persisted = try NativeLibraryReader.readConfig(rootURL: fixture.library, id: config.slug)
        XCTAssertEqual(persisted.memMiB, 8192, "vm.json memMiB stayed stale")
        XCTAssertEqual(persisted.cpuCount, 6, "vm.json cpuCount stayed stale")
        var expected = config
        expected.memMiB = 8192
        expected.cpuCount = 6
        XCTAssertEqual(persisted, expected, "only the resource fields of vm.json may change")

        for (url, before) in [(fixture.launchSpec, launchBefore), (fixture.handoff, handoffBefore)] {
            let after = try fixture.object(url)
            let resources = try XCTUnwrap(after["resources"] as? NSDictionary, url.lastPathComponent)
            XCTAssertEqual(resources["memory"] as? String, "8192", url.lastPathComponent)
            XCTAssertEqual(resources["cpu"] as? String, "6", url.lastPathComponent)
            XCTAssertEqual(resources["display_fps_cap"] as? String, "60", url.lastPathComponent)
            XCTAssertEqual(resources["balloon_device"] as? Bool, true, url.lastPathComponent)
            let otherAfter = after.mutableCopy() as! NSMutableDictionary
            let otherBefore = before.mutableCopy() as! NSMutableDictionary
            otherAfter.removeObject(forKey: "resources")
            otherBefore.removeObject(forKey: "resources")
            XCTAssertEqual(otherAfter, otherBefore, "non-resource keys of \(url.lastPathComponent) must be preserved")
        }
    }

    /// The Advanced > Resources path itself, with the backend the library factory builds.
    @MainActor
    func testControlModelApplyUpdatesLibraryInventoryForStoppedVM() async throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        try fixture.prepare()
        XCTAssertTrue(VMLibrary.save(fixture.config, rootURL: fixture.library))
        let model = ControlModel(config: fixture.config, backend: fixture.config.makeBackend(libraryRoot: fixture.library),
                                 startsAutomatically: false)
        model.pendingMemGiB = 8
        model.pendingCPU = 6
        model.applyResources()
        for _ in 0..<200 where model.busy { try await Task.sleep(nanoseconds: 10_000_000) }
        XCTAssertFalse(model.busy)
        let listed = try XCTUnwrap(VMLibrary.scan(rootURL: fixture.library).configs.first { $0.slug == fixture.slug })
        XCTAssertEqual(listed.memMiB, 8192)
        XCTAssertEqual(listed.cpuCount, 6)
        let record = try XCTUnwrap(NativeLibraryReader.snapshot(rootURL: fixture.library, id: fixture.slug).records.first)
        XCTAssertEqual(record.memoryMiB, 8192)
        XCTAssertEqual(record.cpuCount, 6)
    }

    func testUnsavableLibraryRegistrationRefusesWithoutTouchingBundle() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        try fixture.prepare()
        // A regular file where the library directory should be: vm.json cannot be saved.
        try Data("blocked".utf8).write(to: fixture.library)
        let launchBefore = try Data(contentsOf: fixture.launchSpec)
        let handoffBefore = try Data(contentsOf: fixture.handoff)
        let backend = fixture.config.makeBackend(libraryRoot: fixture.library)

        XCTAssertFalse(backend.setResources(memMiB: 8192, cpu: 6))

        assertNoGlobalLibraryLeak(fixture)
        XCTAssertEqual(try Data(contentsOf: fixture.launchSpec), launchBefore)
        XCTAssertEqual(try Data(contentsOf: fixture.handoff), handoffBefore)
        XCTAssertEqual(backend.resources().memMiB, 4096)
        XCTAssertEqual(backend.resources().cpu, 2)
    }

    func testUnreadableLaunchSpecRefusesBeforeAnyWrite() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        try fixture.prepare()
        try FileManager.default.removeItem(at: fixture.launchSpec)
        XCTAssertTrue(VMLibrary.save(fixture.config, rootURL: fixture.library))
        let registration = fixture.library.appendingPathComponent(fixture.slug).appendingPathComponent("vm.json")
        let registrationBefore = try Data(contentsOf: registration)
        let handoffBefore = try Data(contentsOf: fixture.handoff)

        XCTAssertFalse(fixture.config.makeBackend(libraryRoot: fixture.library).setResources(memMiB: 8192, cpu: 6))

        XCTAssertEqual(try Data(contentsOf: registration), registrationBefore)
        XCTAssertEqual(try Data(contentsOf: fixture.handoff), handoffBefore)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.launchSpec.path))
    }

    func testHandoffWithoutResourcesRefusesBeforeAnyWrite() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        try fixture.prepare(handoffResources: false)
        XCTAssertTrue(VMLibrary.save(fixture.config, rootURL: fixture.library))
        let registration = fixture.library.appendingPathComponent(fixture.slug).appendingPathComponent("vm.json")
        let registrationBefore = try Data(contentsOf: registration)
        let launchBefore = try Data(contentsOf: fixture.launchSpec)
        let handoffBefore = try Data(contentsOf: fixture.handoff)

        XCTAssertFalse(fixture.config.makeBackend(libraryRoot: fixture.library).setResources(memMiB: 8192, cpu: 6))

        XCTAssertEqual(try Data(contentsOf: registration), registrationBefore)
        XCTAssertEqual(try Data(contentsOf: fixture.launchSpec), launchBefore)
        XCTAssertEqual(try Data(contentsOf: fixture.handoff), handoffBefore)
    }

    func testFailedBundleWriteRestoresLibraryRegistration() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        try fixture.prepare()
        XCTAssertTrue(VMLibrary.save(fixture.config, rootURL: fixture.library))
        // Readable but not writable: the preflight succeeds and the bundle write fails.
        try FileManager.default.setAttributes([.posixPermissions: 0o555], ofItemAtPath: fixture.metadata.path)
        let launchBefore = try Data(contentsOf: fixture.launchSpec)
        let handoffBefore = try Data(contentsOf: fixture.handoff)
        let backend = fixture.config.makeBackend(libraryRoot: fixture.library)

        XCTAssertFalse(backend.setResources(memMiB: 8192, cpu: 6))

        let persisted = try NativeLibraryReader.readConfig(rootURL: fixture.library, id: fixture.slug)
        XCTAssertEqual(persisted.memMiB, 4096)
        XCTAssertEqual(persisted.cpuCount, 2)
        XCTAssertEqual(try Data(contentsOf: fixture.launchSpec), launchBefore)
        XCTAssertEqual(try Data(contentsOf: fixture.handoff), handoffBefore)
        XCTAssertEqual(backend.resources().memMiB, 4096)
    }
}
