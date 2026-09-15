import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallStoreFixture {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("install-store-" + UUID().uuidString)
    let validator = HvfWindowsInstallValidationProbe()
    var jobs: [@MainActor () async -> Void] = []
    var made = 0

    func library() -> LibraryModel {
        LibraryModel(rootURL: root, migrateLegacy: false, installSessionFactory: { plan in
            self.made += 1
            return HvfWindowsInstallSession(plan: plan, validate: { [probe = self.validator] in
                probe.validate($0)
            }, schedule: { self.jobs.append($0) })
        })
    }

    func save(_ config: VMConfig, diskGiB: Int = 64) throws {
        let request = HvfWindowsInstallRequest(isoPath: root.appendingPathComponent("absent.iso").path,
            isoSHA256: String(repeating: "a", count: 64), diskGiB: diskGiB, injectViogpu3d: false)
        XCTAssertTrue(request.save(bundlePath: config.bundlePath))
        XCTAssertTrue(VMLibrary.save(config, rootURL: root))
    }

    func config(_ slug: String = "windows") -> VMConfig {
        VMConfig(id: slug, name: slug, displayName: slug, backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: root.appendingPathComponent(slug + "/bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: slug, displayWidth: 1280, displayHeight: 720, installPending: true)
    }

    func removeRegistration(_ config: VMConfig) throws {
        try FileManager.default.removeItem(at: root.appendingPathComponent(config.slug + "/vm.json"))
    }

    func finishCancelled(_ session: HvfWindowsInstallSession) async {
        session.cancel()
        let job = jobs.removeFirst()
        await job() // Only the existing pre-dispatch cancellation guard is exercised.
        XCTAssertFalse(session.isRunning)
    }

    func clean() {
        jobs.removeAll() // Queued pipeline work never executes during teardown.
        try? FileManager.default.removeItem(at: root)
    }
}
