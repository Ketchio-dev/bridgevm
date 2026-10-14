import XCTest
@testable import BridgeVMControl

@MainActor
final class GenericControlRetentionTests: XCTestCase {
    func testFastVZActiveControlSurvivesUnrelatedVMAdoption() throws {
        try assertRetention(kind: "fast-vz")
    }

    func testQemuActiveControlSurvivesUnrelatedVMAdoption() throws {
        try assertRetention(kind: "qemu-compat")
    }

    private func assertRetention(kind: String) throws {
        for missing in [false, true] {
            let base = ProcessInfo.processInfo.environment["TMPDIR"] ?? NSTemporaryDirectory()
            let root = URL(fileURLWithPath: base).appendingPathComponent("generic-retention-" + UUID().uuidString)
            try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
            defer { try? FileManager.default.removeItem(at: root) }
            let backend = GenericRetentionBackend()
            let config = configuration("active", kind: kind, root: root)
            let other = configuration("other", kind: kind, root: root)
            XCTAssertTrue(VMLibrary.save(config, rootURL: root))
            let library = LibraryModel(rootURL: root, migrateLegacy: false, modelFactory: {
                ControlModel(config: $0, backend: backend, startsAutomatically: false)
            })
            library.selectedID = config.slug
            weak var accepted: ControlModel?
            do {
                let model = library.model(for: config)
                model.running = true // Existing observed process; no launch, lookup, or stop.
                accepted = model
                XCTAssertTrue(library.selectedPaletteCommands(dismiss: {}).contains { $0.title == "정지: active" })
            }
            let registration = root.appendingPathComponent("active/vm.json")
            if missing { try FileManager.default.removeItem(at: registration) }
            else { try Data("{invalid".utf8).write(to: registration) }
            XCTAssertTrue(VMLibrary.save(other, rootURL: root))

            XCTAssertTrue(library.add(other), "Normal create/import completion rescans the library")

            XCTAssertEqual(library.selectedID, other.slug)
            XCTAssertNil(library.vms.first { $0.slug == config.slug })
            XCTAssertNotNil(accepted, "An already active control must remain owned after its registration is unreadable")
            XCTAssertNotNil(library.retainedControlRecords.first { $0.descriptor.config == config },
                "Current retained navigation must offer the existing active control after selection changes")
            XCTAssertEqual(backend.calls, 0)
            if !missing { XCTAssertEqual(try Data(contentsOf: registration), Data("{invalid".utf8)) }
        }
    }

    private func configuration(_ name: String, kind: String, root: URL) -> VMConfig {
        VMConfig(id: name, name: name, displayName: name, backendKind: kind,
            bootMode: kind == "fast-vz" ? "direct-kernel" : "windows-uefi",
            bundlePath: root.appendingPathComponent(name + "/bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
            guestName: name, displayWidth: 1280, displayHeight: 800)
    }
}

private final class GenericRetentionBackend: VMBackend {
    var calls = 0
    let displayName = "owned fixture", kind = "fixture"
    let supportsGuestCommands = false, supportsPackageInstall = false
    let supportsClipboard = false, supportsSSH = false, supportsResourceChanges = false
    func isRunning() -> Bool { calls += 1; return false }
    func currentIP() -> String? { calls += 1; return nil }
    func start() -> Bool { calls += 1; return false }
    func stop() { calls += 1 }
    func resources() -> (memMiB: Int, cpu: Int) { calls += 1; return (4096, 2) }
    func setResources(memMiB: Int, cpu: Int) -> Bool { calls += 1; return false }
    func runInGuest(_ command: String) -> (output: String, code: Int32) { calls += 1; return ("", -1) }
}
