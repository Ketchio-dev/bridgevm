#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
extension AppUIInstallCancelFixtureTests {
    func withFixture(_ body: @MainActor (AppUIInstallCancelFixture, [String: Data]) async throws -> Void) async throws {
        // Honor the caller's SSD TMPDIR instead of Foundation's cached temporaryDirectory.
        let temporary = try XCTUnwrap(ProcessInfo.processInfo.environment["TMPDIR"])
        guard temporary.hasPrefix("/"), !temporary.contains("\0") else { throw AppUIDriverFailure.invalidFile }
        let parent = URL(fileURLWithPath: temporary, isDirectory: true).resolvingSymlinksInPath()
        let root = parent.appendingPathComponent("install-cancel-fixture-" + UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false,
            attributes: [.posixPermissions: 0o700])
        let fixture: AppUIInstallCancelFixture
        do { fixture = try AppUIInstallCancelFixture(root: root) }
        catch { try? FileManager.default.removeItem(at: root); throw error }
        var failure: Error?
        do { try await body(fixture, bytes(fixture)) }
        catch { failure = error }
        let settled = await fixture.settle()
        XCTAssertTrue(settled, "Owned validation must settle before its fixture is removed")
        if settled { try FileManager.default.removeItem(at: root) }
        if let failure { throw failure }
    }

    func bytes(_ fixture: AppUIInstallCancelFixture) throws -> [String: Data] {
        let names = ["pending/vm.json", "pending/bundle/metadata/hvf-install.json",
            "pending/bundle/repo/helpers/wimlib-imagex", "pending/bundle/repo/helpers/bridgevm-catalog-verify"]
        return try Dictionary(uniqueKeysWithValues: names.map { name in
            (name, try Data(contentsOf: fixture.root.appendingPathComponent(name)))
        })
    }

    func waitForValidation(_ fixture: AppUIInstallCancelFixture) async throws -> Bool {
        let deadline = ProcessInfo.processInfo.systemUptime + 5
        while ProcessInfo.processInfo.systemUptime < deadline {
            if fixture.counters.snapshot["validation_calls"] == 1 { return true }
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        return false
    }

    func assertCounters(_ fixture: AppUIInstallCancelFixture, calls: Int, finished: Int,
                        resets: Int, cancels: Int, file: StaticString = #filePath, line: UInt = #line) {
        var expected = Dictionary(uniqueKeysWithValues: AppUIInstallCancelCounters.names.map { ($0, 0) })
        expected["plan_builds"] = 1; expected["model_creations"] = 1; expected["install_creations"] = 1
        expected["validation_calls"] = calls; expected["validation_finished"] = finished
        expected["process_resets"] = resets; expected["process_cancels"] = cancels
        XCTAssertEqual(fixture.counters.snapshot, expected,
            "No backend, runtime, process launch, pipeline, media, source, finalization or completion effect", file: file, line: line)
    }

    func assertPreserved(_ fixture: AppUIInstallCancelFixture, _ before: [String: Data],
                         file: StaticString = #filePath, line: UInt = #line) throws {
        XCTAssertEqual(try bytes(fixture), before, "All four authored files retain exact bytes", file: file, line: line)
        XCTAssertTrue(try fixture.preserved(), "No extra fixture paths or changed hashes", file: file, line: line)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.root.appendingPathComponent("absent.iso").path),
            "The authored ISO remains absent", file: file, line: line)
    }
}
#endif
