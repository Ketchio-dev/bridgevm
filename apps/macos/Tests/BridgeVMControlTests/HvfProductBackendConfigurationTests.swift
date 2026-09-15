import XCTest
@testable import BridgeVMControl

final class HvfProductBackendConfigurationTests: XCTestCase {
    private final class AccessRecorder: VTPMStateKeyProviding {
        var processLookups = 0
        var keyRequests = 0

        func stateKey(for stableVMID: String, allowCreation: Bool) throws -> Data {
            keyRequests += 1
            XCTFail("Configuration inspection must not request a vTPM key")
            throw CocoaError(.fileReadUnknown)
        }
    }

    private struct Fixture {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("backend-config-" + UUID().uuidString)
        var library: URL { root.appendingPathComponent("library") }

        func config(memory: Int? = nil, cpu: Int? = nil) -> VMConfig {
            let name = root.lastPathComponent
            return VMConfig(id: name, name: name, displayName: name, backendKind: "hvf-engine",
                bootMode: "windows-hvf", bundlePath: root.appendingPathComponent("bundle").path,
                runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                leasesPath: "", guestName: name, displayWidth: 1280, displayHeight: 720,
                memMiB: memory, cpuCount: cpu)
        }

        func backend(_ config: VMConfig, access: AccessRecorder) -> HvfWindowsBackend {
            HvfWindowsBackend(config, processIsRunning: { _ in
                access.processLookups += 1
                return false
            }, libraryRoot: library, vtpmKeyProvider: access)
        }
    }

    func testPureConversionRetainsInjectedLibraryRootWithoutRuntimeAccess() throws {
        let fixture = Fixture()
        let config = fixture.config()
        let access = AccessRecorder()
        let backend = fixture.backend(config, access: access)

        let converted = backend.makeHvfEngineConfig()
        let context = try XCTUnwrap(converted.libraryContext)

        XCTAssertEqual(context.rootURL, fixture.library)
        XCTAssertEqual(context.config, config)
        XCTAssertEqual(access.processLookups, 0)
        XCTAssertEqual(access.keyRequests, 0)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.root.path))
    }

    func testCachedConversionObservesPendingJournalInItsOwningLibrary() throws {
        let fixture = Fixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let original = fixture.config()
        let access = AccessRecorder()
        defer {
            XCTAssertEqual(access.processLookups, 0)
            XCTAssertEqual(access.keyRequests, 0)
        }
        XCTAssertTrue(VMLibrary.save(original, rootURL: fixture.library))
        let backend = fixture.backend(original, access: access)
        let context = try XCTUnwrap(backend.makeHvfEngineConfig().libraryContext)
        XCTAssertTrue(context.readinessIssues.isEmpty)
        var moved = original
        moved.bundlePath = fixture.root.appendingPathComponent("destination/bundle").path

        let journal = try VMRelocationJournal.begin(original: original, moved: moved, rootURL: fixture.library)

        XCTAssertTrue(VMRelocationJournal.isPending(original, rootURL: fixture.library))
        XCTAssertTrue(context.readinessIssues.contains { $0.code == "relocation-pending" },
                      "A cached conversion must consult its owning library's current journal")
        try FileManager.default.removeItem(at: journal)
        XCTAssertTrue(context.readinessIssues.isEmpty)
        // Only ordinary registration/journal metadata exists; no guest assets.
        XCTAssertFalse(FileManager.default.fileExists(atPath: original.bundlePath))
        XCTAssertFalse(FileManager.default.fileExists(atPath: moved.bundlePath))
    }

    func testReportedImplicitAndMixedResourcesMatchLaunchConfiguration() {
        let cases: [(memory: Int?, cpu: Int?)] = [(nil, nil), (8192, nil), (nil, 8)]
        for values in cases {
            let fixture = Fixture()
            let config = fixture.config(memory: values.memory, cpu: values.cpu)
            let access = AccessRecorder()
            let backend = fixture.backend(config, access: access)
            let launch = backend.makeHvfEngineConfig()

            let reported = backend.resources()

            XCTAssertEqual(reported.memMiB, launch.ramMiB, "memory=\(String(describing: values.memory))")
            XCTAssertEqual(reported.cpu, launch.smpCpus, "cpu=\(String(describing: values.cpu))")
            XCTAssertEqual(backend.config, config, "Reporting must not persist default values")
            XCTAssertEqual(access.processLookups, 0)
            XCTAssertEqual(access.keyRequests, 0)
            XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.root.path))
        }
    }

    func testReportedExplicitResourcesRemainExact() {
        let fixture = Fixture()
        let config = fixture.config(memory: 8192, cpu: 16)
        let access = AccessRecorder()
        let backend = fixture.backend(config, access: access)

        let reported = backend.resources()
        let launch = backend.makeHvfEngineConfig()

        XCTAssertEqual(reported.memMiB, 8192)
        XCTAssertEqual(reported.cpu, 16)
        XCTAssertEqual(launch.ramMiB, reported.memMiB)
        XCTAssertEqual(launch.smpCpus, reported.cpu)
        XCTAssertEqual(backend.config, config)
        XCTAssertEqual(access.processLookups, 0)
        XCTAssertEqual(access.keyRequests, 0)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.root.path))
    }

    func testPendingInstallFallbackKeepsExistingLaunchValues() {
        let cases: [(memory: Int?, cpu: Int?)] = [(nil, nil), (8192, 8)]
        for values in cases {
            let fixture = Fixture()
            var config = fixture.config(memory: values.memory, cpu: values.cpu)
            config.installPending = true
            let access = AccessRecorder()
            let backend = fixture.backend(config, access: access)

            let launch = backend.makeHvfEngineConfig()
            let reported = backend.resources()

            XCTAssertNil(launch.libraryContext)
            XCTAssertEqual(launch.ramMiB, values.memory ?? 6144)
            XCTAssertEqual(launch.smpCpus, values.cpu ?? 4)
            XCTAssertEqual(reported.memMiB, launch.ramMiB)
            XCTAssertEqual(reported.cpu, launch.smpCpus)
            XCTAssertEqual(backend.config, config)
            XCTAssertEqual(access.processLookups, 0)
            XCTAssertEqual(access.keyRequests, 0)
            XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.root.path))
        }
    }
}
