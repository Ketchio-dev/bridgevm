@testable import BridgeVMApp
import Foundation
import XCTest

@MainActor
final class AppSettingsTests: XCTestCase {
    func testDaemonEndpointDefaultSocketMatchesCliStoreConvention() {
        XCTAssertEqual(
            DaemonEndpoint.defaultSocketPath(environment: [
                "BRIDGEVM_HOME": "/tmp/bridgevm-home",
                "HOME": "/Users/example",
            ]),
            "/tmp/bridgevm-home/run/bridgevmd.sock"
        )
        XCTAssertEqual(
            DaemonEndpoint.defaultSocketPath(environment: ["HOME": "/Users/example"]),
            "/Users/example/.bridgevm/run/bridgevmd.sock"
        )
        XCTAssertEqual(
            DaemonEndpoint.defaultSocketPath(environment: [:]),
            ".bridgevm/run/bridgevmd.sock"
        )
    }

    func testSettingsPersistDaemonSocketPathMockToggleAndAppleVzLiveStartToggle() {
        let suiteName = "BridgeVMAppTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer {
            defaults.removePersistentDomain(forName: suiteName)
        }

        let settings = AppSettings(defaults: defaults)
        XCTAssertFalse(settings.hasPendingChanges)
        XCTAssertFalse(settings.allowAppleVzRealStart)

        settings.daemonSocketPath = "/tmp/bridgevmd.sock"
        settings.useMockInventory = true
        settings.allowAppleVzRealStart = true
        XCTAssertTrue(settings.hasPendingChanges)

        let reloaded = AppSettings(defaults: defaults)
        XCTAssertEqual(reloaded.daemonSocketPath, "/tmp/bridgevmd.sock")
        XCTAssertTrue(reloaded.useMockInventory)
        XCTAssertTrue(reloaded.allowAppleVzRealStart)
        XCTAssertFalse(reloaded.hasPendingChanges)
    }

    func testPersistedDefaultsDriveBundledDaemonSupervisorLaunchEnvironment() throws {
        let suiteName = "BridgeVMAppTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer {
            defaults.removePersistentDomain(forName: suiteName)
        }
        defaults.set("", forKey: "bridgevm.daemonSocketPath")
        defaults.set(false, forKey: "bridgevm.useMockInventory")
        defaults.set(true, forKey: "bridgevm.allowAppleVzRealStart")

        let settings = AppSettings(defaults: defaults)
        XCTAssertFalse(settings.useMockInventory)
        XCTAssertTrue(settings.allowAppleVzRealStart)
        XCTAssertEqual(settings.effectiveDaemonSocketPath, DaemonEndpoint.local.socketPath)
        XCTAssertFalse(settings.hasPendingChanges)

        let supervisor = BundledDaemonSupervisor()
        var launchedProcess: Process?
        var launchedEnvironment: [String: String] = [:]

        let report = supervisor.startIfNeeded(
            settings: settings,
            helperResolver: { name in
                URL(fileURLWithPath: "/tmp/BridgeVM.app/Contents/Helpers/\(name)")
            },
            launcher: { _, environment in
                launchedEnvironment = environment
                let process = Process()
                process.executableURL = URL(fileURLWithPath: "/bin/sleep")
                process.arguments = ["60"]
                try process.run()
                launchedProcess = process
                return BundledDaemonProcess(process: process)
            },
            livenessProbeDelay: 0,
            socketReadyProbe: { _ in true }
        )
        defer {
            _ = supervisor.stop(timeout: 1.0)
            if launchedProcess?.isRunning == true {
                launchedProcess?.terminate()
            }
        }

        XCTAssertEqual(report.state, .running)
        XCTAssertEqual(report.socketPath, DaemonEndpoint.local.socketPath)
        XCTAssertEqual(launchedEnvironment["BRIDGEVM_APPLE_VZ_ALLOW_REAL_START"], "1")
    }

    func testSettingsTrackPendingChangesAgainstAppliedSnapshot() {
        let suiteName = "BridgeVMAppTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer {
            defaults.removePersistentDomain(forName: suiteName)
        }

        let settings = AppSettings(defaults: defaults)
        settings.daemonSocketPath = "/tmp/bridgevmd.sock"

        XCTAssertTrue(settings.hasPendingChanges)

        settings.markApplied()
        XCTAssertFalse(settings.hasPendingChanges)

        settings.useMockInventory = true
        XCTAssertTrue(settings.hasPendingChanges)

        settings.markApplied()
        settings.allowAppleVzRealStart = true
        XCTAssertTrue(settings.hasPendingChanges)
    }

    func testSettingsValidateDaemonSocketPathUnlessMockInventoryIsEnabled() {
        let suiteName = "BridgeVMAppTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer {
            defaults.removePersistentDomain(forName: suiteName)
        }

        let settings = AppSettings(defaults: defaults)
        settings.daemonSocketPath = "   "

        XCTAssertTrue(settings.hasValidDaemonSettings)
        XCTAssertEqual(settings.effectiveDaemonSocketPath, DaemonEndpoint.local.socketPath)
        XCTAssertTrue(settings.daemonModeSummary.contains("Using default bridgevmd socket"))

        settings.daemonSocketPath = "  /tmp/bridgevmd.sock  "
        XCTAssertTrue(settings.hasValidDaemonSettings)
        XCTAssertEqual(settings.effectiveDaemonSocketPath, "/tmp/bridgevmd.sock")
        XCTAssertTrue(settings.daemonModeSummary.contains("Connection errors are shown"))

        settings.daemonSocketPath = ""
        settings.useMockInventory = true
        XCTAssertTrue(settings.hasValidDaemonSettings)
        XCTAssertTrue(settings.daemonModeSummary.contains("Using mock inventory"))

        settings.useMockInventory = false
        settings.allowAppleVzRealStart = true
        XCTAssertTrue(settings.daemonModeSummary.contains("Apple VZ live starts are enabled"))
    }

    func testBundledDaemonEnvironmentUsesBundledRunnerHelpersWhenPresent() {
        let environment = BundledDaemonSupervisor.daemonEnvironment(
            base: ["PATH": "/usr/bin"],
            helperResolver: { name in
                URL(fileURLWithPath: "/tmp/BridgeVM.app/Contents/Helpers/\(name)")
            }
        )

        XCTAssertEqual(environment["PATH"], "/usr/bin")
        XCTAssertEqual(
            environment["BRIDGEVM_LIGHTVM_RUNNER"],
            "/tmp/BridgeVM.app/Contents/Helpers/lightvm-runner"
        )
        XCTAssertEqual(
            environment["BRIDGEVM_APPLE_VZ_RUNNER"],
            "/tmp/BridgeVM.app/Contents/Helpers/AppleVzRunner"
        )
        XCTAssertNil(environment["BRIDGEVM_APPLE_VZ_ALLOW_REAL_START"])
    }

    func testBundledDaemonEnvironmentAllowsAppleVzRealStartOnlyWhenEnabled() {
        let disabled = BundledDaemonSupervisor.daemonEnvironment(
            base: ["BRIDGEVM_APPLE_VZ_ALLOW_REAL_START": "1"],
            helperResolver: { name in
                URL(fileURLWithPath: "/tmp/BridgeVM.app/Contents/Helpers/\(name)")
            },
            allowAppleVzRealStart: false
        )
        XCTAssertNil(disabled["BRIDGEVM_APPLE_VZ_ALLOW_REAL_START"])

        let enabled = BundledDaemonSupervisor.daemonEnvironment(
            base: [:],
            helperResolver: { name in
                URL(fileURLWithPath: "/tmp/BridgeVM.app/Contents/Helpers/\(name)")
            },
            allowAppleVzRealStart: true
        )
        XCTAssertEqual(enabled["BRIDGEVM_APPLE_VZ_ALLOW_REAL_START"], "1")
    }

    func testBundledDaemonSupervisorReportsSkippedMockLaunch() {
        let settings = AppSettings(defaults: isolatedDefaults())
        settings.useMockInventory = true
        let supervisor = BundledDaemonSupervisor()

        let report = supervisor.startIfNeeded(
            settings: settings,
            helperResolver: { _ in XCTFail("mock mode should not resolve helpers"); return nil },
            launcher: { _, _ in
                XCTFail("mock mode should not launch")
                return BundledDaemonProcess(process: Process())
            }
        )

        XCTAssertEqual(report.state, .disabledByMockInventory)
        XCTAssertTrue(report.isHealthy)
        XCTAssertNil(report.helperPath)
        XCTAssertEqual(report.socketPath, DaemonEndpoint.local.socketPath)
    }

    func testBundledDaemonSupervisorReportsCustomSocketPathWhenLaunchIsSkipped() {
        let settings = AppSettings(defaults: isolatedDefaults())
        settings.daemonSocketPath = "  /tmp/bridgevm-custom.sock  "
        let supervisor = BundledDaemonSupervisor()

        let report = supervisor.startIfNeeded(
            settings: settings,
            helperResolver: { _ in XCTFail("custom socket should not resolve helpers"); return nil },
            launcher: { _, _ in
                XCTFail("custom socket should not launch")
                return BundledDaemonProcess(process: Process())
            }
        )

        XCTAssertEqual(report.state, .customSocket)
        XCTAssertTrue(report.isHealthy)
        XCTAssertNil(report.helperPath)
        XCTAssertEqual(report.socketPath, "/tmp/bridgevm-custom.sock")
    }

    func testBundledDaemonDiagnosticsSummaryIncludesFailurePathsAndDaemonOutput() {
        let report = BundledDaemonLaunchReport(
            state: .failed,
            helperPath: "/tmp/BridgeVM.app/Contents/Helpers/bridgevmd",
            socketPath: "/tmp/bridgevmd.sock",
            detail: "Bundled bridgevmd failed to launch: launcher refused",
            stderrTail: "\nbridgevmd stderr before launch failure\n"
        )

        let summary = BundledDaemonDiagnosticsSummary(report: report)

        XCTAssertFalse(summary.isHealthy)
        XCTAssertEqual(
            summary.statusText,
            "Bundled bridgevmd failed to launch: launcher refused"
        )
        XCTAssertEqual(summary.helperPath, "/tmp/BridgeVM.app/Contents/Helpers/bridgevmd")
        XCTAssertEqual(summary.socketPath, "/tmp/bridgevmd.sock")
        XCTAssertEqual(summary.stderrPreview, "bridgevmd stderr before launch failure")
    }

    func testBundledDaemonDiagnosticsSummaryOmitsEmptyDaemonOutput() {
        let report = BundledDaemonLaunchReport(
            state: .customSocket,
            helperPath: nil,
            socketPath: "/tmp/bridgevm-custom.sock",
            detail: "Bundled daemon launch skipped because a custom daemon socket is configured.",
            stderrTail: " \n "
        )

        let summary = BundledDaemonDiagnosticsSummary(report: report)

        XCTAssertTrue(summary.isHealthy)
        XCTAssertEqual(
            summary.statusText,
            "Bundled daemon launch skipped because a custom daemon socket is configured."
        )
        XCTAssertNil(summary.helperPath)
        XCTAssertEqual(summary.socketPath, "/tmp/bridgevm-custom.sock")
        XCTAssertNil(summary.stderrPreview)
    }

    func testBundledDaemonDiagnosticsSummaryKeepsMockSocketContext() {
        let report = BundledDaemonLaunchReport(
            state: .disabledByMockInventory,
            helperPath: nil,
            socketPath: DaemonEndpoint.local.socketPath,
            detail: "Bundled daemon launch skipped because mock inventory is enabled.",
            stderrTail: nil
        )

        let summary = BundledDaemonDiagnosticsSummary(report: report)

        XCTAssertTrue(summary.isHealthy)
        XCTAssertEqual(summary.socketPath, DaemonEndpoint.local.socketPath)
        XCTAssertNil(summary.helperPath)
        XCTAssertNil(summary.stderrPreview)
    }

    func testBundledDaemonSupervisorPassesAppleVzLiveStartEnvironment() throws {
        let settings = AppSettings(defaults: isolatedDefaults())
        settings.allowAppleVzRealStart = true
        let supervisor = BundledDaemonSupervisor()
        var launchedProcess: Process?
        var launchedEnvironment: [String: String] = [:]

        let report = supervisor.startIfNeeded(
            settings: settings,
            helperResolver: { name in
                URL(fileURLWithPath: "/tmp/BridgeVM.app/Contents/Helpers/\(name)")
            },
            launcher: { _, environment in
                launchedEnvironment = environment
                let process = Process()
                process.executableURL = URL(fileURLWithPath: "/bin/sleep")
                process.arguments = ["60"]
                try process.run()
                launchedProcess = process
                return BundledDaemonProcess(process: process)
            },
            livenessProbeDelay: 0,
            socketReadyProbe: { _ in true }
        )
        defer {
            if launchedProcess?.isRunning == true {
                launchedProcess?.terminate()
            }
        }

        XCTAssertEqual(report.state, .running)
        XCTAssertEqual(report.socketPath, DaemonEndpoint.local.socketPath)
        XCTAssertEqual(launchedEnvironment["BRIDGEVM_APPLE_VZ_ALLOW_REAL_START"], "1")
        XCTAssertTrue(supervisor.stop(timeout: 1.0))
    }

    func testBundledDaemonSupervisorRelaunchesWhenAppleVzLiveStartSettingChanges() throws {
        let settings = AppSettings(defaults: isolatedDefaults())
        let supervisor = BundledDaemonSupervisor()
        var launchedProcesses: [Process] = []
        var launchedEnvironments: [[String: String]] = []

        func launchSleep() throws -> BundledDaemonProcess {
            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/bin/sleep")
            process.arguments = ["60"]
            try process.run()
            launchedProcesses.append(process)
            return BundledDaemonProcess(process: process)
        }

        let firstReport = supervisor.startIfNeeded(
            settings: settings,
            helperResolver: { name in
                URL(fileURLWithPath: "/tmp/BridgeVM.app/Contents/Helpers/\(name)")
            },
            launcher: { _, environment in
                launchedEnvironments.append(environment)
                return try launchSleep()
            },
            livenessProbeDelay: 0,
            socketReadyProbe: { _ in true }
        )
        settings.allowAppleVzRealStart = true
        let secondReport = supervisor.startIfNeeded(
            settings: settings,
            helperResolver: { name in
                URL(fileURLWithPath: "/tmp/BridgeVM.app/Contents/Helpers/\(name)")
            },
            launcher: { _, environment in
                launchedEnvironments.append(environment)
                return try launchSleep()
            },
            livenessProbeDelay: 0,
            socketReadyProbe: { _ in true }
        )
        defer {
            _ = supervisor.stop(timeout: 1.0)
            for process in launchedProcesses where process.isRunning {
                process.terminate()
            }
        }

        XCTAssertEqual(firstReport.state, .running)
        XCTAssertEqual(secondReport.state, .running)
        XCTAssertEqual(firstReport.socketPath, DaemonEndpoint.local.socketPath)
        XCTAssertEqual(secondReport.socketPath, DaemonEndpoint.local.socketPath)
        XCTAssertEqual(launchedProcesses.count, 2)
        XCTAssertFalse(launchedProcesses[0].isRunning)
        XCTAssertNil(launchedEnvironments[0]["BRIDGEVM_APPLE_VZ_ALLOW_REAL_START"])
        XCTAssertEqual(launchedEnvironments[1]["BRIDGEVM_APPLE_VZ_ALLOW_REAL_START"], "1")
    }

    func testBundledDaemonSupervisorReportsMissingBridgevmdHelper() {
        let settings = AppSettings(defaults: isolatedDefaults())
        let supervisor = BundledDaemonSupervisor()

        let report = supervisor.startIfNeeded(
            settings: settings,
            helperResolver: { _ in nil },
            launcher: { _, _ in
                XCTFail("missing helper should not launch")
                return BundledDaemonProcess(process: Process())
            }
        )

        XCTAssertEqual(report.state, .missingHelper)
        XCTAssertFalse(report.isHealthy)
        XCTAssertEqual(report.socketPath, DaemonEndpoint.local.socketPath)
        XCTAssertTrue(report.detail.contains("missing"))
    }

    func testBundledDaemonSupervisorReportsLaunchFailureWithHelperPath() {
        let settings = AppSettings(defaults: isolatedDefaults())
        let supervisor = BundledDaemonSupervisor()
        let helperURL = URL(fileURLWithPath: "/tmp/BridgeVM.app/Contents/Helpers/bridgevmd")

        let report = supervisor.startIfNeeded(
            settings: settings,
            helperResolver: { name in
                URL(fileURLWithPath: "/tmp/BridgeVM.app/Contents/Helpers/\(name)")
            },
            launcher: { _, _ in
                throw BundledDaemonLaunchError(
                    message: "launcher refused",
                    stderrTail: "bridgevmd stderr before launch failure"
                )
            }
        )

        XCTAssertEqual(report.state, .failed)
        XCTAssertFalse(report.isHealthy)
        XCTAssertEqual(report.helperPath, helperURL.path)
        XCTAssertEqual(report.socketPath, DaemonEndpoint.local.socketPath)
        XCTAssertTrue(report.detail.contains("launcher refused"))
        XCTAssertEqual(report.stderrTail, "bridgevmd stderr before launch failure")
    }

    func testBundledDaemonSupervisorReportsImmediateExitWithHelperPath() {
        let settings = AppSettings(defaults: isolatedDefaults())
        let supervisor = BundledDaemonSupervisor()
        let helperURL = URL(fileURLWithPath: "/tmp/BridgeVM.app/Contents/Helpers/bridgevmd")

        let report = supervisor.startIfNeeded(
            settings: settings,
            helperResolver: { name in
                URL(fileURLWithPath: "/tmp/BridgeVM.app/Contents/Helpers/\(name)")
            },
            launcher: { _, _ in
                BundledDaemonProcess(
                    process: Process(),
                    stderrTail: { "bridgevmd stderr before immediate exit" }
                )
            },
            livenessProbeDelay: 0,
            socketReadyProbe: { _ in true }
        )

        XCTAssertEqual(report.state, .failed)
        XCTAssertFalse(report.isHealthy)
        XCTAssertEqual(report.helperPath, helperURL.path)
        XCTAssertEqual(report.socketPath, DaemonEndpoint.local.socketPath)
        XCTAssertTrue(report.detail.contains("exited immediately"))
        XCTAssertEqual(report.stderrTail, "bridgevmd stderr before immediate exit")
    }

    func testBundledDaemonSupervisorWaitsForSocketReadiness() throws {
        let settings = AppSettings(defaults: isolatedDefaults())
        let supervisor = BundledDaemonSupervisor()
        var launchedProcess: Process?
        var probeCount = 0

        let report = supervisor.startIfNeeded(
            settings: settings,
            helperResolver: { name in
                URL(fileURLWithPath: "/tmp/BridgeVM.app/Contents/Helpers/\(name)")
            },
            launcher: { _, _ in
                let process = Process()
                process.executableURL = URL(fileURLWithPath: "/bin/sleep")
                process.arguments = ["60"]
                try process.run()
                launchedProcess = process
                return BundledDaemonProcess(process: process)
            },
            livenessProbeDelay: 0,
            socketReadinessTimeout: 0.5,
            socketReadinessPollInterval: 0.001,
            socketReadyProbe: { path in
                XCTAssertEqual(path, DaemonEndpoint.local.socketPath)
                probeCount += 1
                return probeCount >= 3
            }
        )
        defer {
            _ = supervisor.stop(timeout: 1.0)
            if launchedProcess?.isRunning == true {
                launchedProcess?.terminate()
            }
        }

        XCTAssertEqual(report.state, .running)
        XCTAssertTrue(report.isHealthy)
        XCTAssertEqual(report.socketPath, DaemonEndpoint.local.socketPath)
        XCTAssertGreaterThanOrEqual(probeCount, 3)
        XCTAssertTrue(report.detail.contains("socket is ready"))
    }

    func testBundledDaemonSupervisorFailsWhenSocketNeverBecomesReady() throws {
        let settings = AppSettings(defaults: isolatedDefaults())
        let supervisor = BundledDaemonSupervisor()
        var launchedProcess: Process?

        let report = supervisor.startIfNeeded(
            settings: settings,
            helperResolver: { name in
                URL(fileURLWithPath: "/tmp/BridgeVM.app/Contents/Helpers/\(name)")
            },
            launcher: { _, _ in
                let process = Process()
                process.executableURL = URL(fileURLWithPath: "/bin/sleep")
                process.arguments = ["60"]
                try process.run()
                launchedProcess = process
                return BundledDaemonProcess(
                    process: process,
                    stderrTail: { "bridgevmd still starting" }
                )
            },
            livenessProbeDelay: 0,
            socketReadinessTimeout: 0.02,
            socketReadinessPollInterval: 0.001,
            socketReadyProbe: { _ in false }
        )
        defer {
            if launchedProcess?.isRunning == true {
                launchedProcess?.terminate()
            }
        }

        XCTAssertEqual(report.state, .failed)
        XCTAssertFalse(report.isHealthy)
        XCTAssertEqual(report.socketPath, DaemonEndpoint.local.socketPath)
        XCTAssertTrue(report.detail.contains("ready socket before timeout"))
        XCTAssertEqual(report.stderrTail, "bridgevmd still starting")
        XCTAssertFalse(launchedProcess?.isRunning == true)
    }

    func testBundledDaemonProcessCapturesStdoutTail() throws {
        let launched = try BundledDaemonSupervisor.runDaemonProcess(
            executableURL: URL(fileURLWithPath: "/usr/bin/yes"),
            environment: [:]
        )
        defer {
            launched.cleanup()
        }

        Thread.sleep(forTimeInterval: 0.05)
        launched.process.terminate()
        launched.process.waitUntilExit()
        Thread.sleep(forTimeInterval: 0.05)

        let tail = try XCTUnwrap(launched.stderrTail())
        XCTAssertTrue(tail.contains("y"))
    }

    func testBundledDaemonSupervisorStopTerminatesLaunchedHelper() throws {
        let settings = AppSettings(defaults: isolatedDefaults())
        let supervisor = BundledDaemonSupervisor()
        var launchedProcess: Process?

        let report = supervisor.startIfNeeded(
            settings: settings,
            helperResolver: { name in
                URL(fileURLWithPath: "/tmp/BridgeVM.app/Contents/Helpers/\(name)")
            },
            launcher: { _, _ in
                let process = Process()
                process.executableURL = URL(fileURLWithPath: "/bin/sleep")
                process.arguments = ["60"]
                try process.run()
                launchedProcess = process
                return BundledDaemonProcess(process: process)
            },
            livenessProbeDelay: 0,
            socketReadyProbe: { _ in true }
        )
        defer {
            if launchedProcess?.isRunning == true {
                launchedProcess?.terminate()
            }
        }

        XCTAssertEqual(report.state, .running)
        XCTAssertEqual(report.socketPath, DaemonEndpoint.local.socketPath)
        XCTAssertTrue(launchedProcess?.isRunning == true)

        XCTAssertTrue(supervisor.stop(timeout: 1.0))
        XCTAssertFalse(launchedProcess?.isRunning == true)
    }

    func testBundledDaemonSupervisorStopKillsHelperThatIgnoresGracefulSignals() throws {
        let settings = AppSettings(defaults: isolatedDefaults())
        let supervisor = BundledDaemonSupervisor()
        var launchedProcess: Process?

        let report = supervisor.startIfNeeded(
            settings: settings,
            helperResolver: { name in
                URL(fileURLWithPath: "/tmp/BridgeVM.app/Contents/Helpers/\(name)")
            },
            launcher: { _, _ in
                let process = Process()
                process.executableURL = URL(fileURLWithPath: "/bin/sh")
                process.arguments = ["-c", "trap '' TERM INT; exec /bin/sleep 60"]
                try process.run()
                launchedProcess = process
                return BundledDaemonProcess(process: process)
            },
            livenessProbeDelay: 0.05,
            socketReadyProbe: { _ in true }
        )
        defer {
            if let launchedProcess, launchedProcess.isRunning {
                kill(launchedProcess.processIdentifier, SIGKILL)
                launchedProcess.waitUntilExit()
            }
        }

        XCTAssertEqual(report.state, .running)
        XCTAssertTrue(launchedProcess?.isRunning == true)
        XCTAssertTrue(supervisor.stop(timeout: 0.05))
        XCTAssertFalse(launchedProcess?.isRunning == true)
    }

    func testAppModelApplySettingsSwapsDashboardClientAndReloads() async throws {
        let suiteName = "BridgeVMAppTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer {
            defaults.removePersistentDomain(forName: suiteName)
        }
        let settings = AppSettings(defaults: defaults)
        settings.useMockInventory = true
        let model = BridgeVMAppModel(settings: settings)

        model.applySettings()
        try await Task.sleep(nanoseconds: 300_000_000)

        XCTAssertFalse(model.dashboardModel.virtualMachines.isEmpty)
    }

    func testAppModelDaemonModeDoesNotFallbackToMockInventoryWhenSocketIsUnavailable()
        async throws
    {
        let suiteName = "BridgeVMAppTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer {
            defaults.removePersistentDomain(forName: suiteName)
        }
        let settings = AppSettings(defaults: defaults)
        settings.useMockInventory = false
        settings.daemonSocketPath =
            "/tmp/bridgevm-tests/missing-\(UUID().uuidString)/bridgevmd.sock"
        let model = BridgeVMAppModel(settings: settings)

        await model.dashboardModel.load()

        XCTAssertTrue(model.dashboardModel.virtualMachines.isEmpty)
        XCTAssertNotNil(model.dashboardModel.lastRefreshError)
        XCTAssertEqual(model.dashboardModel.inventorySourceTitle, "Not loaded")
        XCTAssertEqual(model.bundledDaemonLaunchReport?.state, .customSocket)
    }

    func testAppModelStoreDoctorUsesMockModeWithoutLoadingInventory() async throws {
        let suiteName = "BridgeVMAppTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer {
            defaults.removePersistentDomain(forName: suiteName)
        }
        let settings = AppSettings(defaults: defaults)
        settings.useMockInventory = true
        let model = BridgeVMAppModel(settings: settings)

        model.checkStoreDoctor()
        try await Task.sleep(nanoseconds: 200_000_000)

        guard case let .ready(report) = model.storeDoctorState else {
            XCTFail("expected ready store doctor state")
            return
        }

        XCTAssertEqual(report.status, "MOCK")
        XCTAssertEqual(report.source, "Mock inventory")
        XCTAssertTrue(model.dashboardModel.virtualMachines.isEmpty)
    }

    func testAppModelApplySettingsIgnoresStaleStoreDoctorResult() async throws {
        let suiteName = "BridgeVMAppTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer {
            defaults.removePersistentDomain(forName: suiteName)
        }
        let settings = AppSettings(defaults: defaults)
        settings.useMockInventory = true
        let doctorClient = DelayedStoreDoctorClient()
        let model = BridgeVMAppModel(
            settings: settings,
            doctorClientFactory: { _ in doctorClient }
        )

        model.checkStoreDoctor()
        await doctorClient.waitUntilRequested()
        model.applySettings()
        await doctorClient.complete(
            StoreDoctorReport(
                storeRoot: "/stale/store",
                vmsDir: "/stale/store/vms",
                status: "OK",
                source: "stale doctor"
            )
        )
        try await Task.sleep(nanoseconds: 100_000_000)

        XCTAssertEqual(model.storeDoctorState, .idle)
    }

    func testAppModelApplySettingsIgnoresStaleInFlightDashboardLoad() async throws {
        let suiteName = "BridgeVMAppTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer {
            defaults.removePersistentDomain(forName: suiteName)
        }
        let settings = AppSettings(defaults: defaults)
        settings.useMockInventory = true
        let oldVirtualMachine = testVirtualMachine(name: "Old VM")
        let newVirtualMachine = testVirtualMachine(name: "New VM")
        let oldClient = DelayedInventoryClient(
            sourceTitle: "Old inventory",
            virtualMachines: [oldVirtualMachine]
        )
        let newClient = DelayedInventoryClient(
            sourceTitle: "New inventory",
            virtualMachines: [newVirtualMachine]
        )
        var clients: [DelayedInventoryClient] = [oldClient, newClient]
        let model = BridgeVMAppModel(
            settings: settings,
            clientFactory: { _ in
                clients.removeFirst()
            }
        )

        let oldLoad = Task {
            await model.dashboardModel.load()
        }
        await oldClient.waitUntilRequested()

        settings.allowAppleVzRealStart = true
        model.applySettings()
        await newClient.waitUntilRequested()
        await newClient.complete()
        try await Task.sleep(nanoseconds: 100_000_000)

        await oldClient.complete()
        await oldLoad.value
        try await Task.sleep(nanoseconds: 100_000_000)

        XCTAssertEqual(model.dashboardModel.virtualMachines, [newVirtualMachine])
        XCTAssertEqual(model.dashboardModel.inventorySourceTitle, "New inventory")
        XCTAssertEqual(model.dashboardModel.selection, newVirtualMachine.id)
        XCTAssertNil(model.dashboardModel.lastRefreshError)
        XCTAssertFalse(model.dashboardModel.isLoading)
        XCTAssertFalse(settings.hasPendingChanges)
    }

    private func isolatedDefaults() -> UserDefaults {
        let suiteName = "BridgeVMAppTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        addTeardownBlock {
            defaults.removePersistentDomain(forName: suiteName)
        }
        return defaults
    }

    private func testVirtualMachine(name: String) -> VirtualMachine {
        VirtualMachine(
            id: UUID(),
            name: name,
            guest: "Ubuntu 24.04",
            status: .stopped,
            mode: .fast,
            resources: .init(cpuCount: 2, memoryGB: 4, diskGB: 40),
            uptime: "-",
            ipAddress: nil,
            lastStarted: nil,
            notes: ""
        )
    }
}

private actor DelayedStoreDoctorClient: StoreDoctorInspecting {
    private var requestContinuation: CheckedContinuation<StoreDoctorReport, Error>?
    private var waiter: CheckedContinuation<Void, Never>?

    func inspectStoreDoctor() async throws -> StoreDoctorReport {
        try await withCheckedThrowingContinuation { continuation in
            requestContinuation = continuation
            waiter?.resume()
            waiter = nil
        }
    }

    func waitUntilRequested() async {
        if requestContinuation != nil {
            return
        }
        await withCheckedContinuation { continuation in
            waiter = continuation
        }
    }

    func complete(_ report: StoreDoctorReport) {
        requestContinuation?.resume(returning: report)
        requestContinuation = nil
    }
}

private actor DelayedInventoryClient: VirtualMachineClient, VirtualMachineClientSourceProviding {
    nonisolated let sourceTitle: String
    nonisolated let allowsMutationsForCurrentInventory = true
    private let virtualMachines: [VirtualMachine]
    private var requestContinuation: CheckedContinuation<[VirtualMachine], Error>?
    private var waiter: CheckedContinuation<Void, Never>?

    init(sourceTitle: String, virtualMachines: [VirtualMachine]) {
        self.sourceTitle = sourceTitle
        self.virtualMachines = virtualMachines
    }

    func listVirtualMachines() async throws -> [VirtualMachine] {
        try await withCheckedThrowingContinuation { continuation in
            requestContinuation = continuation
            waiter?.resume()
            waiter = nil
        }
    }

    func waitUntilRequested() async {
        if requestContinuation != nil {
            return
        }
        await withCheckedContinuation { continuation in
            waiter = continuation
        }
    }

    func complete() {
        requestContinuation?.resume(returning: virtualMachines)
        requestContinuation = nil
    }

    func listBootTemplates() async throws -> [BootTemplate] {
        []
    }

    func inspectBootMediaStatus(on id: VirtualMachine.ID) async throws -> BootMediaStatus {
        throw VirtualMachineClientError.daemonResponseInvalid
    }

    func importBootMedia(
        sourcePath: String,
        kind: BootMediaStatusEntry.Kind?,
        on id: VirtualMachine.ID
    ) async throws -> BootMediaImportMetadata {
        throw VirtualMachineClientError.daemonResponseInvalid
    }

    func verifyBootMedia(
        expectedSHA256: String,
        kind: BootMediaStatusEntry.Kind?,
        on id: VirtualMachine.ID
    ) async throws -> BootMediaVerificationMetadata {
        throw VirtualMachineClientError.daemonResponseInvalid
    }

    func planBootMediaDownload(
        url: String,
        expectedSHA256: String?,
        kind: BootMediaStatusEntry.Kind?,
        on id: VirtualMachine.ID
    ) async throws -> BootMediaDownloadPlanMetadata {
        throw VirtualMachineClientError.daemonResponseInvalid
    }

    func downloadBootMedia(
        kind: BootMediaStatusEntry.Kind?,
        on id: VirtualMachine.ID
    ) async throws -> BootMediaDownloadResultMetadata {
        throw VirtualMachineClientError.daemonResponseInvalid
    }

    func inspectGuestToolsStatus(on id: VirtualMachine.ID) async throws -> GuestToolsStatus {
        throw VirtualMachineClientError.daemonResponseInvalid
    }

    func sendGuestToolsCommand(
        _ command: GuestToolsAgentCommand,
        requestID: String?,
        on id: VirtualMachine.ID
    ) async throws -> GuestToolsCommandDispatch {
        throw VirtualMachineClientError.daemonResponseInvalid
    }

    func inspectSnapshotPreflightStatus(on id: VirtualMachine.ID) async throws
        -> SnapshotPreflightStatus
    {
        throw VirtualMachineClientError.daemonResponseInvalid
    }

    func executeApplicationConsistentSnapshot(
        named snapshotName: String,
        freezeTimeoutMillis: UInt64?,
        on id: VirtualMachine.ID
    ) async throws -> ApplicationConsistentSnapshotExecution {
        throw VirtualMachineClientError.daemonResponseInvalid
    }

    func inspectQMPStatus(on id: VirtualMachine.ID) async throws -> QMPStatus {
        throw VirtualMachineClientError.daemonResponseInvalid
    }

    func viewLogs(kind: VMLogKind, bytes: UInt64?, on id: VirtualMachine.ID) async throws
        -> VMLogView
    {
        throw VirtualMachineClientError.daemonResponseInvalid
    }

    func inspectRunnerStatus(on id: VirtualMachine.ID) async throws -> RunnerStatus? {
        throw VirtualMachineClientError.daemonResponseInvalid
    }

    func createVirtualMachine(_ request: CreateVirtualMachineRequest) async throws
        -> VirtualMachine
    {
        throw VirtualMachineClientError.daemonResponseInvalid
    }

    func cloneVirtualMachine(on id: VirtualMachine.ID, newName: String, linked: Bool) async throws
        -> CloneVirtualMachineMetadata
    {
        throw VirtualMachineClientError.daemonResponseInvalid
    }

    func perform(_ action: VirtualMachineAction, on id: VirtualMachine.ID) async throws
        -> VMActionResult
    {
        throw VirtualMachineClientError.daemonResponseInvalid
    }
}
