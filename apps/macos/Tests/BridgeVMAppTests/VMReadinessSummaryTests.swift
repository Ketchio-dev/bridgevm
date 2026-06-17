import XCTest
@testable import BridgeVMApp

final class VMReadinessSummaryTests: XCTestCase {
  func testRequestsBootMediaCheckBeforeOtherPreparation() {
    let summary = VMReadinessSummary.evaluate(
      virtualMachine: makeVirtualMachine(status: .stopped),
      bootMediaStatus: nil,
      bootMediaStatusError: nil,
      runnerStatus: nil,
      runnerStatusError: nil,
      snapshotChain: nil,
      snapshotChainError: nil,
      diskPreparation: nil,
      diskCreation: nil,
      diskInspection: nil,
      diskVerification: nil
    )

    XCTAssertEqual(summary.title, "Check boot media")
    XCTAssertEqual(summary.action, .refreshBootMedia)
    XCTAssertEqual(summary.severity, .informational)
  }

  func testRequestsDiskPreparationAfterBootMediaExists() {
    let summary = VMReadinessSummary.evaluate(
      virtualMachine: makeVirtualMachine(status: .stopped),
      bootMediaStatus: makeBootMediaStatus(exists: true),
      bootMediaStatusError: nil,
      runnerStatus: nil,
      runnerStatusError: nil,
      snapshotChain: nil,
      snapshotChainError: nil,
      diskPreparation: nil,
      diskCreation: nil,
      diskInspection: nil,
      diskVerification: nil
    )

    XCTAssertEqual(summary.title, "Prepare primary disk")
    XCTAssertEqual(summary.action, .prepareDisk)
    XCTAssertEqual(summary.severity, .attention)
  }

  func testRequestsLaunchPreparationAfterDiskIsPrepared() {
    let summary = VMReadinessSummary.evaluate(
      virtualMachine: makeVirtualMachine(status: .stopped),
      bootMediaStatus: makeBootMediaStatus(exists: true),
      bootMediaStatusError: nil,
      runnerStatus: nil,
      runnerStatusError: nil,
      snapshotChain: nil,
      snapshotChainError: nil,
      diskPreparation: makeDiskPreparation(),
      diskCreation: nil,
      diskInspection: nil,
      diskVerification: nil
    )

    XCTAssertEqual(summary.title, "Prepare launch")
    XCTAssertEqual(summary.action, .prepareRun)
    XCTAssertEqual(summary.severity, .informational)
  }

  func testReadyLaunchUsesPrimaryAction() {
    let summary = VMReadinessSummary.evaluate(
      virtualMachine: makeVirtualMachine(status: .stopped),
      bootMediaStatus: makeBootMediaStatus(exists: true),
      bootMediaStatusError: nil,
      runnerStatus: makeRunnerStatus(ready: true),
      runnerStatusError: nil,
      snapshotChain: nil,
      snapshotChainError: nil,
      diskPreparation: makeDiskPreparation(),
      diskCreation: nil,
      diskInspection: nil,
      diskVerification: nil
    )

    XCTAssertEqual(summary.title, "Launch checks clear")
    XCTAssertEqual(summary.actionTitle, "Start")
    XCTAssertEqual(summary.action, .primaryAction)
    XCTAssertEqual(summary.severity, .ready)
  }

  func testBlockedLaunchSummaryIncludesBlockerCodeMessageAndCapability() {
    let summary = VMReadinessSummary.evaluate(
      virtualMachine: makeVirtualMachine(status: .stopped),
      bootMediaStatus: makeBootMediaStatus(exists: true),
      bootMediaStatusError: nil,
      runnerStatus: RunnerStatus(
        engine: "fullvm",
        pid: nil,
        command: ["qemu-system-x86_64"],
        logPath: "logs/qemu.log",
        startedAtUnix: 1_710_000_000,
        dryRun: true,
        launchSpecPath: nil,
        launchReadiness: LaunchReadiness(
          ready: false,
          blockers: [
            LaunchReadinessBlocker(
              code: "qemu-host-only-requires-privilege",
              message:
                "Compatibility Mode QEMU host-only networking uses vmnet-host, which requires the qemu process to run as root or carry the com.apple.vm.networking entitlement",
              path: nil,
              capability: "qemu-network"
            )
          ]
        )
      ),
      runnerStatusError: nil,
      snapshotChain: nil,
      snapshotChainError: nil,
      diskPreparation: makeDiskPreparation(),
      diskCreation: nil,
      diskInspection: nil,
      diskVerification: nil
    )

    XCTAssertEqual(summary.title, "Blocked (1)")
    XCTAssertEqual(summary.action, .prepareRun)
    XCTAssertEqual(summary.severity, .blocked)
    XCTAssertEqual(
      summary.detail,
      "qemu-host-only-requires-privilege: Compatibility Mode QEMU host-only networking uses vmnet-host, which requires the qemu process to run as root or carry the com.apple.vm.networking entitlement (qemu-network)")
  }

  func testRunningVMOffersQMPProbe() {
    let summary = VMReadinessSummary.evaluate(
      virtualMachine: makeVirtualMachine(status: .running),
      bootMediaStatus: nil,
      bootMediaStatusError: nil,
      runnerStatus: nil,
      runnerStatusError: nil,
      snapshotChain: nil,
      snapshotChainError: nil,
      diskPreparation: nil,
      diskCreation: nil,
      diskInspection: nil,
      diskVerification: nil
    )

    XCTAssertEqual(summary.title, "Console diagnostics available")
    XCTAssertEqual(summary.actionTitle, "Probe QMP")
    XCTAssertEqual(summary.action, .openConsole)
    XCTAssertEqual(summary.severity, .ready)
  }

  func testConsoleCapabilityIsDiagnosticOnlyUntilViewerExists() {
    let runningCapability = ConsoleCapability.evaluate(for: makeVirtualMachine(status: .running))
    let stoppedCapability = ConsoleCapability.evaluate(for: makeVirtualMachine(status: .stopped))

    XCTAssertFalse(runningCapability.graphicalViewerAvailable)
    XCTAssertTrue(runningCapability.qmpDiagnosticsAvailable)
    XCTAssertTrue(runningCapability.boundedLogTailsAvailable)
    XCTAssertEqual(runningCapability.title, "Diagnostics only")
    XCTAssertEqual(runningCapability.graphicalViewerTitle, "Not embedded")
    XCTAssertEqual(runningCapability.qmpDiagnosticsTitle, "Probe available")
    XCTAssertEqual(runningCapability.actionTitle, "Probe QMP")
    XCTAssertEqual(runningCapability.actionSystemImage, "point.3.connected.trianglepath.dotted")

    XCTAssertFalse(stoppedCapability.graphicalViewerAvailable)
    XCTAssertFalse(stoppedCapability.qmpDiagnosticsAvailable)
    XCTAssertTrue(stoppedCapability.boundedLogTailsAvailable)
    XCTAssertEqual(stoppedCapability.qmpDiagnosticsTitle, "Requires running VM")
  }

  func testConsoleCapabilityAdvertisesGraphicalViewerWhenQemuPlanHasVNCEndpoint() {
    let plan = QemuLaunchPlan(
      program: "qemu-system-x86_64",
      args: ["-display", "vnc=:0"]
    )
    let capability = ConsoleCapability.evaluate(
      for: makeVirtualMachine(status: .running),
      qemuLaunchPlan: plan
    )

    XCTAssertTrue(capability.graphicalViewerAvailable)
    XCTAssertTrue(capability.qmpDiagnosticsAvailable)
    XCTAssertEqual(capability.title, "Graphical console advertised")
    XCTAssertEqual(capability.graphicalViewerTitle, "Available")
    XCTAssertEqual(capability.actionTitle, "Open VNC")
    XCTAssertEqual(capability.actionSystemImage, "display")
    XCTAssertEqual(
      capability.detail,
      "Verify viewer output separately from QMP diagnostics and bounded logs.")
  }

  func testMetadataOnlyReportWithoutBlockersAndLiveRequirementUsesEvidenceRequiredTitle() {
    let report = VMReadinessReport(
      vm: "Dev VM",
      mode: .fast,
      state: .stopped,
      metadataOnly: true,
      liveE2ERequired: true,
      evidenceRequirements: [],
      bootMedia: nil,
      bootMediaError: nil,
      snapshotChain: nil,
      snapshotChainError: nil,
      runner: nil,
      runnerError: nil,
      preRunLaunchReadiness: nil,
      blockers: [],
      notes: []
    )

    XCTAssertEqual(report.readinessTitle, "Live E2E evidence required")
  }

  func testReadinessReportSummarizesPendingRequiredEvidence() {
    let report = VMReadinessReport(
      vm: "Dev VM",
      mode: .fast,
      state: .stopped,
      metadataOnly: true,
      liveE2ERequired: true,
      evidenceRequirements: [
        VMEvidenceRequirement(
          kind: "live-boot",
          required: true,
          proven: false,
          note: "No live boot transcript has been captured."
        ),
        VMEvidenceRequirement(
          kind: "console",
          required: true,
          proven: false,
          note: "No graphical console evidence has been captured."
        ),
        VMEvidenceRequirement(
          kind: "guest-tools-effects",
          required: true,
          proven: true,
          note: "Synthetic proof only."
        ),
      ],
      bootMedia: nil,
      bootMediaError: nil,
      snapshotChain: nil,
      snapshotChainError: nil,
      runner: nil,
      runnerError: nil,
      preRunLaunchReadiness: nil,
      blockers: [],
      notes: []
    )

    XCTAssertEqual(report.pendingRequiredEvidence.map(\.kind), ["live-boot", "console"])
    XCTAssertEqual(report.pendingRequiredEvidence.map(\.title), ["Live boot", "Console"])
    XCTAssertEqual(report.evidenceReadinessTitle, "2 evidence checks pending")
  }

  private func makeVirtualMachine(status: VirtualMachine.Status) -> VirtualMachine {
    VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: status,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "0m",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
  }

  private func makeBootMediaStatus(exists: Bool) -> BootMediaStatus {
    BootMediaStatus(
      vm: "dev",
      entries: [
        BootMediaStatusEntry(
          kind: .installerImage,
          path: "media/ubuntu.iso",
          exists: exists,
          sizeBytes: exists ? 1024 : nil,
          lastImport: nil,
          lastVerification: nil,
          lastDownloadPlan: nil,
          lastDownload: nil
        )
      ]
    )
  }

  private func makeDiskPreparation() -> DiskPreparation {
    DiskPreparation(
      path: "disks/root.qcow2",
      format: "qcow2",
      size: "64G",
      sizeBytes: 64 * 1024 * 1024 * 1024,
      exists: true,
      created: false,
      createCommand: nil,
      preparedAtUnix: 1_710_000_000
    )
  }

  private func makeRunnerStatus(ready: Bool) -> RunnerStatus {
    RunnerStatus(
      engine: "lightvm",
      pid: nil,
      command: ["lightvm", "run", "dev"],
      logPath: "logs/runner.log",
      startedAtUnix: 1_710_000_000,
      dryRun: true,
      launchSpecPath: "metadata/launch.json",
      launchReadiness: LaunchReadiness(ready: ready, blockers: [])
    )
  }
}
