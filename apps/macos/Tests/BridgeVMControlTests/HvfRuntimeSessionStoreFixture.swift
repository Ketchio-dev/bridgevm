import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeSessionStoreFixture {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("runtime-store-" + UUID().uuidString)
    var repoRoot: URL { root.appendingPathComponent("owned-install-repo") }
    var runtimeConfigs: [HvfEngineConfig] = []
    var installCount = 0
    var processLookups = 0
    var installJobs: [@MainActor () async -> Void] = []
    var preserveForUnsettledPreparation = false

    init() throws {
        let helpers = repoRoot.appendingPathComponent("helpers")
        try FileManager.default.createDirectory(at: helpers, withIntermediateDirectories: true)
        for name in ["wimlib-imagex", "bridgevm-catalog-verify"] {
            let path = helpers.appendingPathComponent(name)
            try Data("owned marker; never executed\n".utf8).write(to: path)
            try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: path.path)
        }
        guard HvfWindowsWimlib.resolve(repoRoot: repoRoot) == helpers.appendingPathComponent("wimlib-imagex").path,
              HvfWindowsCatalogVerifier.resolve(repoRoot: repoRoot) == helpers.appendingPathComponent("bridgevm-catalog-verify").path else {
            XCTFail("Stop before preparation if helper resolution leaves the owned fixture")
            throw CocoaError(.fileReadUnknown)
        }
    }

    func library() -> LibraryModel {
        LibraryModel(rootURL: root, migrateLegacy: false, installSessionFactory: { plan in
            self.installCount += 1
            return HvfWindowsInstallSession(plan: plan, validate: { _ in nil },
                schedule: { self.installJobs.append($0) })
        }, runtimeSessionFactory: { config in
            self.runtimeConfigs.append(config)
            return HvfEngineSession(config: config, repoRoot: self.root,
                processIsRunning: { _ in self.processLookups += 1; return false })
        }, installPreparation: .init(repoRoot: repoRoot), modelFactory: { ControlModel(config: $0, startsAutomatically: false) })
    }

    func config(_ slug: String = "windows") -> VMConfig {
        VMConfig(id: slug, name: slug, displayName: slug, backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: root.appendingPathComponent(slug + "/bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: slug, displayWidth: 1280, displayHeight: 720, installPending: false)
    }

    func save(_ config: VMConfig) {
        XCTAssertTrue(VMLibrary.save(config, rootURL: root))
        let request = HvfWindowsInstallRequest(isoPath: root.appendingPathComponent("absent.iso").path,
            isoSHA256: String(repeating: "a", count: 64), diskGiB: 64, injectViogpu3d: false)
        XCTAssertTrue(request.save(bundlePath: config.bundlePath))
    }

    func removeRegistration(_ config: VMConfig) throws {
        try FileManager.default.removeItem(at: root.appendingPathComponent(config.slug + "/vm.json"))
    }

    func clean() {
        XCTAssertEqual(processLookups, 0, "Constructing detail values must not attach or launch")
        installJobs.removeAll() // No queued install work is ever executed.
        if !preserveForUnsettledPreparation { try? FileManager.default.removeItem(at: root) }
    }
}
