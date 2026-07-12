import XCTest
@testable import BridgeVMControl

@MainActor
final class ControlModelPackageTests: XCTestCase {
    func testPackageInstallCommandQuotesEveryValidatedPackage() {
        let command = ControlModel.packageInstallCommand(["git", "libc6:arm64", "g++"])

        XCTAssertEqual(
            command,
            "sudo DEBIAN_FRONTEND=noninteractive apt-get install -y 'git' 'libc6:arm64' 'g++' 2>&1 | tail -25"
        )
    }

    func testPackageInstallCommandRejectsEmptyAndOptionLikeRequests() {
        XCTAssertNil(ControlModel.packageInstallCommand([]))
        XCTAssertNil(ControlModel.packageInstallCommand(["  "]))
        XCTAssertNil(ControlModel.packageInstallCommand(["--allow-unauthenticated"]))
    }

    func testPackageInstallCommandRejectsShellSyntax() {
        XCTAssertNil(ControlModel.packageInstallCommand(["git; touch /tmp/injected"]))
        XCTAssertNil(ControlModel.packageInstallCommand(["$(id)"]))
        XCTAssertNil(ControlModel.packageInstallCommand(["git\nreboot"]))
    }

    func testInvalidRequestDoesNotReachBackend() {
        let backend = PackageBackend()
        let model = ControlModel(config: makeConfig(), backend: backend, startsAutomatically: false)
        model.running = true

        model.installPackages(["git; reboot"], label: "Invalid")

        XCTAssertFalse(model.busy)
        XCTAssertEqual(backend.commands, [])
        XCTAssertTrue(model.softwareLog.contains("올바르지 않습니다"))
    }

    private func makeConfig() -> VMConfig {
        VMConfig(id: "package-test", name: "Package Test", displayName: "Package Test",
                 backendKind: "fast-vz", bootMode: nil, bundlePath: "", runnerPath: "",
                 launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                 leasesPath: "", guestName: "", displayWidth: 1, displayHeight: 1)
    }
}

private final class PackageBackend: VMBackend {
    let displayName = "Package Test"
    let kind = "test"
    let supportsGuestCommands = true
    let supportsPackageInstall = true
    let supportsClipboard = false
    let supportsSSH = false
    let supportsResourceChanges = false
    private(set) var commands: [String] = []

    func isRunning() -> Bool { true }
    func currentIP() -> String? { nil }
    func start() -> Bool { true }
    func stop() {}
    func resources() -> (memMiB: Int, cpu: Int) { (4096, 2) }
    func setResources(memMiB: Int, cpu: Int) -> Bool { false }
    func runInGuest(_ command: String) -> (output: String, code: Int32) {
        commands.append(command)
        return ("", 0)
    }
}
