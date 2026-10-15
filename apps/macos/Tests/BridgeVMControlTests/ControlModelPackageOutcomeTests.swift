import XCTest
@testable import BridgeVMControl

@MainActor
final class ControlModelPackageOutcomeTests: XCTestCase {
    func testGeneratedCommandPreservesUpstreamFailureAndLastTwentyFiveLines() throws {
        let fixture = try PackageCommandFixture(status: 100)
        let command = try XCTUnwrap(ControlModel.packageInstallCommand(["git", "libc6:arm64", "g++"]))
        let result = fixture.run(command)
        XCTAssertEqual(result.code, 100)
        XCTAssertEqual(result.output, fixture.expectedOutput)
        XCTAssertEqual(try fixture.arguments(), ["DEBIAN_FRONTEND=noninteractive", "apt-get", "install", "-y",
                                                "git", "libc6:arm64", "g++"])
        try fixture.assertUnchanged()
    }

    func testActualCompletionDoesNotCallFailedPackageInstallSuccessful() async throws {
        let fixture = try PackageCommandFixture(status: 100)
        let backend = PackageCommandBackend(fixture)
        let model = model(backend)
        model.installPackages(["git"], label: "Synthetic")
        XCTAssertTrue(model.busy)
        await waitForCompletion(model)
        XCTAssertEqual(backend.calls, 1)
        XCTAssertTrue(model.softwareLog.contains(fixture.expectedOutput))
        XCTAssertTrue(model.softwareLog.contains("❌ 실패 (exit 100)"))
        XCTAssertFalse(model.softwareLog.contains("✅ 설치 완료"))
        try fixture.assertUnchanged()
    }

    func testSuccessfulCommandAndActualCompletionRetainSuccess() async throws {
        let fixture = try PackageCommandFixture(status: 0)
        let backend = PackageCommandBackend(fixture)
        let model = model(backend)
        model.installPackages(["git"], label: "Synthetic")
        await waitForCompletion(model)
        XCTAssertEqual(backend.calls, 1)
        XCTAssertTrue(model.softwareLog.contains(fixture.expectedOutput))
        XCTAssertTrue(model.softwareLog.contains("✅ 설치 완료: Synthetic"))
        XCTAssertFalse(model.softwareLog.contains("❌ 실패"))
        XCTAssertEqual(try fixture.arguments(), ["DEBIAN_FRONTEND=noninteractive", "apt-get", "install", "-y", "git"])
        try fixture.assertUnchanged()
    }

    func testUnavailableSudoCannotBecomeSuccessOrFallBackToSystemSudo() throws {
        let fixture = try PackageCommandFixture(status: 0)
        try FileManager.default.removeItem(at: fixture.bin.appendingPathComponent("sudo"))
        let result = fixture.run(try XCTUnwrap(ControlModel.packageInstallCommand(["git"])))
        XCTAssertNotEqual(result.code, 0)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.argumentsFile.path))
    }

    private func model(_ backend: PackageCommandBackend) -> ControlModel {
        let config = VMConfig(id: "synthetic-package", name: "Synthetic", displayName: "Synthetic",
            backendKind: "fast-vz", bootMode: nil, bundlePath: "", runnerPath: "", launchSpecPath: "",
            handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "",
            displayWidth: 1, displayHeight: 1)
        let model = ControlModel(config: config, backend: backend, startsAutomatically: false)
        model.running = true
        return model
    }

    private func waitForCompletion(_ model: ControlModel) async {
        let deadline = ProcessInfo.processInfo.systemUptime + 5
        while model.busy, ProcessInfo.processInfo.systemUptime < deadline {
            try? await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTAssertFalse(model.busy, "owned synthetic command must acknowledge completion")
    }
}
