import Foundation
import XCTest

@testable import BridgeVMApp

@MainActor
final class DashboardViewModelTests: XCTestCase {
  func testLoadRecordsInventorySourceAndRefreshStatus() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    let model = DashboardViewModel(
      client: StubVirtualMachineClient(
        sourceTitle: "Mock inventory",
        listResult: .success([virtualMachine])
      )
    )

    await model.load()

    XCTAssertEqual(model.virtualMachines, [virtualMachine])
    XCTAssertEqual(model.inventorySourceTitle, "Mock inventory")
    XCTAssertNotNil(model.lastRefreshDate)
    XCTAssertNil(model.lastRefreshError)
    XCTAssertTrue(model.refreshStatusText.hasPrefix("Last refreshed"))
  }

  func testMockInventoryFirstReadinessLoadDoesNotRaiseInvalidDaemonResponseAlert()
    async throws
  {
    let model = DashboardViewModel(client: MockVirtualMachineClient())

    await model.load()
    let virtualMachine = try XCTUnwrap(model.selectedVirtualMachine)
    await model.loadReadinessReport(for: virtualMachine)

    XCTAssertNil(model.alertMessage)
    XCTAssertNotNil(model.readinessReport(for: virtualMachine))
    XCTAssertNotNil(model.bootMediaStatus(for: virtualMachine))
    XCTAssertNotNil(model.snapshotChain(for: virtualMachine))
    XCTAssertNotNil(model.runnerStatus(for: virtualMachine))
    XCTAssertNil(model.readinessReportError(for: virtualMachine))
  }

  func testMockPauseRecordsSuspendedState() async throws {
    let client = MockVirtualMachineClient()
    let virtualMachines = try await client.listVirtualMachines()
    let running = try XCTUnwrap(virtualMachines.first { $0.status == .running })

    let result = try await client.perform(.pause, on: running.id)

    XCTAssertEqual(result.virtualMachine.status, .suspended)
    XCTAssertEqual(result.virtualMachine.uptime, "Suspended")
    XCTAssertEqual(result.message, "\(running.name) suspended.")
    let refreshed = try await client.listVirtualMachines()
    XCTAssertEqual(refreshed.first(where: { $0.id == running.id })?.status, .suspended)
  }

  func testMockRuntimeResourceReapplyRecordsPolicy() async throws {
    let client = MockVirtualMachineClient()
    let virtualMachines = try await client.listVirtualMachines()
    let fastVM = try XCTUnwrap(virtualMachines.first { $0.mode == .fast })

    let policy = try await client.reapplyRuntimeResources(
      visibility: .background,
      on: fastVM.id
    )

    XCTAssertEqual(policy.vm, fastVM.name)
    XCTAssertEqual(policy.mode, "fast")
    XCTAssertEqual(policy.visibility, .background)
    XCTAssertEqual(policy.state, fastVM.status.rawValue)
    XCTAssertEqual(policy.memory, "\(max(2, fastVM.resources.memoryGB / 2) * 1024)")
    XCTAssertEqual(policy.cpu, "\(max(1, fastVM.resources.cpuCount / 2))")
    XCTAssertEqual(policy.displayFPSCap, "10")
    XCTAssertFalse(policy.liveApplied)
    XCTAssertEqual(policy.liveApplyBlockers.first?.code, "mock-runtime-control-unavailable")
  }

  func testMockInventorySupportsSurfacedMaintenanceAndDiagnosticsActions() async throws {
    let client = MockVirtualMachineClient()
    let virtualMachines = try await client.listVirtualMachines()
    let stoppedVM = try XCTUnwrap(virtualMachines.first { $0.status == .stopped })
    let runningFastVM = try XCTUnwrap(
      virtualMachines.first { $0.mode == .fast && $0.status == .running }
    )

    let verification = try await client.verifyActiveDisk(on: stoppedVM.id)
    XCTAssertTrue(verification.activeDisk.path.hasSuffix("/disks/root.qcow2"))
    XCTAssertEqual(verification.exitStatus, "exit status: 0")
    XCTAssertTrue(verification.report.contains("check-errors"))

    let compaction = try await client.compactActiveDisk(on: stoppedVM.id)
    XCTAssertEqual(compaction.activeDisk.path, verification.activeDisk.path)
    XCTAssertLessThan(compaction.compactedSizeBytes, compaction.originalSizeBytes)
    XCTAssertTrue(compaction.backupPath.contains(".precompact-"))

    let bundle = try await client.createDiagnosticBundle(
      output: "/tmp/diagnostics",
      on: stoppedVM.id
    )
    XCTAssertEqual(bundle.vm, stoppedVM.name)
    XCTAssertEqual(bundle.output, "/tmp/diagnostics/legacy-linux-qemu-diagnostics")
    XCTAssertTrue(bundle.files.contains("manifest.yaml"))

    let baseline = try await client.createPerformanceBaseline(
      output: "/tmp/perf",
      on: runningFastVM.id
    )
    XCTAssertEqual(baseline.vm, runningFastVM.name)
    XCTAssertEqual(baseline.artifact, "/tmp/perf/performance-baseline.json")
    XCTAssertEqual(baseline.guestTools.connected, true)
    XCTAssertTrue(
      baseline.measurements.contains { $0.name == "guest_benchmark_cpu_iterations" }
    )

    let sample = try await client.createPerformanceSample(
      output: "/tmp/perf",
      artifactBytes: 4_096,
      iterations: 2,
      sync: true,
      on: runningFastVM.id
    )
    XCTAssertEqual(sample.iterationResults.count, 2)
    XCTAssertEqual(sample.artifactBytes, 4_096)
    XCTAssertTrue(sample.sync)
    XCTAssertTrue(
      sample.measurements.contains { $0.name == "guest_benchmark_cpu_ops_per_sec" }
    )

    let deletion = try await client.deleteVirtualMachine(on: stoppedVM.id)
    XCTAssertEqual(deletion.vm, stoppedVM.name)
    XCTAssertTrue(deletion.metadataOnly)
    let remainingVirtualMachines = try await client.listVirtualMachines()
    XCTAssertFalse(remainingVirtualMachines.contains { $0.id == stoppedVM.id })
  }

  func testLoadRecordsRefreshErrorWithoutDiscardingExistingInventory() async throws {
    let existing = VirtualMachine(
      id: UUID(),
      name: "Existing VM",
      guest: "Debian x86_64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 2, memoryGB: 4, diskGB: 40),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "bridgevmd",
      listResult: .success([existing])
    )
    let model = DashboardViewModel(client: client)
    await model.load()

    client.listResult = .failure(TestRefreshError.offline)
    await model.load()

    XCTAssertEqual(model.virtualMachines, [existing])
    XCTAssertEqual(model.lastRefreshError, "Offline")
    XCTAssertEqual(model.alertMessage, "Offline")
    XCTAssertTrue(model.refreshStatusText.hasPrefix("Refresh failed"))
  }

  func testUpdateClientAllowsNewLoadAndIgnoresStaleOlderLoad() async throws {
    let oldVM = VirtualMachine(
      id: UUID(),
      name: "Old VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "old"
    )
    let newVM = VirtualMachine(
      id: UUID(),
      name: "New VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "new"
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([oldVM]),
      listDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([newVM])
    )
    let model = DashboardViewModel(client: oldClient)

    let oldLoad = Task { await model.load() }
    for _ in 0..<100 where !model.isLoading {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    await model.load()
    await oldLoad.value

    XCTAssertEqual(model.virtualMachines, [newVM])
    XCTAssertEqual(model.inventorySourceTitle, "New inventory")
    XCTAssertEqual(model.selection, newVM.id)
    XCTAssertNil(model.lastRefreshError)
    XCTAssertNil(model.alertMessage)
    XCTAssertFalse(model.isLoading)
  }

  func testUpdateClientIgnoresStaleReadinessReport() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let report = primaryActionPreflightReadinessReport(
      virtualMachine: virtualMachine,
      runnerStatus: primaryActionPreflightRunnerStatus(
        launchReadiness: LaunchReadiness(ready: true, blockers: [])
      )
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      readinessReportResult: .success(report),
      readinessReportDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let staleReadiness = Task { await model.loadReadinessReport(for: virtualMachine) }
    for _ in 0..<100 where model.loadingReadinessReportID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    await staleReadiness.value

    XCTAssertNil(model.readinessReport(for: virtualMachine))
    XCTAssertNil(model.runnerStatus(for: virtualMachine))
    XCTAssertNil(model.alertMessage)
    XCTAssertNil(model.loadingReadinessReportID)
  }

  func testUpdateClientIgnoresStalePrepareRunResult() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let status = primaryActionPreflightRunnerStatus(
      launchReadiness: LaunchReadiness(ready: true, blockers: [])
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      prepareRunResult: .success(status),
      prepareRunDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let stalePrepare = Task { await model.prepareRun(for: virtualMachine) }
    for _ in 0..<100 where model.loadingRunnerStatusID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    let didPrepare = await stalePrepare.value

    XCTAssertFalse(didPrepare)
    XCTAssertNil(model.runnerStatus(for: virtualMachine))
    XCTAssertNil(model.runnerStatusError(for: virtualMachine))
    XCTAssertNil(model.alertMessage)
    XCTAssertNil(model.loadingRunnerStatusID)
  }

  func testLoadBootTemplatesDoesNotChangeInventorySourceTitle() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .stopped)
    let template = BootTemplate(
      id: "ubuntu-arm64-installer",
      guestOS: "ubuntu",
      guestVersion: nil,
      guestArch: "arm64",
      mode: .linuxInstaller,
      mediaLabel: "ubuntu arm64 installer image",
      source: "manual",
      installerImage: "installers/ubuntu-arm64.iso",
      kernelPath: nil,
      initrdPath: nil,
      kernelCommandLine: nil,
      macosRestoreImage: nil,
      note: "Place the installer image inside the bundle."
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "VM inventory",
      listResult: .success([virtualMachine]),
      templatesResult: .success([template])
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    client.sourceTitle = "Template inventory"
    await model.loadBootTemplates()

    XCTAssertEqual(model.bootTemplates, [template])
    XCTAssertEqual(model.inventorySourceTitle, "VM inventory")
  }

  func testUpdateClientIgnoresStaleBootMediaStatus() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .stopped)
    let status = BootMediaStatus(
      vm: "Dev VM",
      entries: [
        BootMediaStatusEntry(
          kind: .installerImage,
          path: "installers/ubuntu-arm64.iso",
          exists: true,
          sizeBytes: 14,
          lastImport: nil,
          lastVerification: nil,
          lastDownloadPlan: nil,
          lastDownload: nil
        )
      ]
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      bootMediaStatusResult: .success(status),
      bootMediaStatusDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let staleStatus = Task { await model.loadBootMediaStatus(for: virtualMachine) }
    for _ in 0..<100 where model.loadingBootMediaStatusID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    await staleStatus.value

    XCTAssertNil(model.bootMediaStatus(for: virtualMachine))
    XCTAssertNil(model.bootMediaStatusError(for: virtualMachine))
    XCTAssertNil(model.alertMessage)
    XCTAssertNil(model.loadingBootMediaStatusID)
  }

  func testUpdateClientIgnoresStaleGuestToolsStatus() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(Self.guestToolsStatus(vm: "Dev VM")),
      guestToolsStatusDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let staleStatus = Task { await model.loadGuestToolsStatus(for: virtualMachine) }
    for _ in 0..<100 where model.loadingGuestToolsStatusID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    await staleStatus.value

    XCTAssertNil(model.guestToolsStatus(for: virtualMachine))
    XCTAssertNil(model.guestToolsStatusError(for: virtualMachine))
    XCTAssertNil(model.guestToolsProvisioning(for: virtualMachine))
    XCTAssertNil(model.guestToolsProvisioningError(for: virtualMachine))
    XCTAssertNil(model.alertMessage)
    XCTAssertNil(model.loadingGuestToolsStatusID)
  }

  func testUpdateClientIgnoresStaleGuestToolsProvisioning() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    let token = GuestToolsToken(vm: "Dev VM", createdAtUnix: 1_710_000_100, tokenLength: 32)
    let command = GuestToolsLinuxCommand(
      vm: "Dev VM",
      transport: .device,
      command: ["bridgevm-agent", "--token", "guest-tools-token.json"],
      tokenFile: "guest-tools-token.json",
      capabilities: ["heartbeat"]
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      guestToolsTokenResult: .success(token),
      guestToolsLinuxCommandResults: [
        .device: .success(command),
        .socket: .success(
          GuestToolsLinuxCommand(
            vm: "Dev VM",
            transport: .socket,
            command: ["bridgevm-agent", "--socket"],
            tokenFile: "guest-tools-token.json",
            capabilities: ["heartbeat"]
          )),
      ],
      guestToolsTokenDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let staleProvisioning = Task { await model.loadGuestToolsProvisioning(for: virtualMachine) }
    for _ in 0..<100 where oldClient.inspectedGuestToolsTokenIDs.isEmpty {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    await staleProvisioning.value

    XCTAssertNil(model.guestToolsProvisioning(for: virtualMachine))
    XCTAssertNil(model.guestToolsProvisioningError(for: virtualMachine))
    XCTAssertNil(model.alertMessage)
  }

  func testUpdateClientIgnoresStaleQemuLaunchPlan() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .stopped)
    let plan = QemuLaunchPlan(
      program: "qemu-system-aarch64",
      args: ["-name", "Dev VM", "-display", "vnc=:0"]
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      qemuLaunchPlanResult: .success(plan),
      qemuLaunchPlanDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let stalePlan = Task { await model.loadQemuLaunchPlan(for: virtualMachine) }
    for _ in 0..<100 where model.loadingQemuLaunchPlanID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    await stalePlan.value

    XCTAssertNil(model.qemuLaunchPlan(for: virtualMachine))
    XCTAssertNil(model.qemuLaunchPlanError(for: virtualMachine))
    XCTAssertNil(model.alertMessage)
    XCTAssertNil(model.loadingQemuLaunchPlanID)
  }

  func testUpdateClientIgnoresStaleSnapshotPreflightStatus() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    let status = SnapshotPreflightStatus(
      vm: "Dev VM",
      consistency: .applicationConsistent,
      backendFreezeThawSupported: true,
      guestToolsConnected: true,
      capabilities: ["guest-tools-heartbeat", "filesystem-freeze-preflight"],
      ready: true,
      blockers: [],
      checkedAtUnix: 1_710_000_200
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      snapshotPreflightStatusResult: .success(status),
      snapshotPreflightStatusDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let staleStatus = Task { await model.loadSnapshotPreflightStatus(for: virtualMachine) }
    for _ in 0..<100 where model.loadingSnapshotPreflightStatusID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    await staleStatus.value

    XCTAssertNil(model.snapshotPreflightStatus(for: virtualMachine))
    XCTAssertNil(model.snapshotPreflightStatusError(for: virtualMachine))
    XCTAssertNil(model.alertMessage)
    XCTAssertNil(model.loadingSnapshotPreflightStatusID)
  }

  func testUpdateClientIgnoresStaleSnapshots() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .stopped)
    let snapshots = [
      VMSnapshot(
        name: "before-upgrade",
        kind: .disk,
        createdAtUnix: 1_710_000_100,
        vmState: .stopped
      )
    ]
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      snapshotsResult: .success(snapshots),
      snapshotsDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let staleSnapshots = Task { await model.loadSnapshots(for: virtualMachine) }
    for _ in 0..<100 where model.loadingSnapshotsID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    await staleSnapshots.value

    XCTAssertTrue(model.snapshots(for: virtualMachine).isEmpty)
    XCTAssertNil(model.snapshotError(for: virtualMachine))
    XCTAssertNil(model.alertMessage)
    XCTAssertNil(model.loadingSnapshotsID)
  }

  func testUpdateClientIgnoresStaleSnapshotChain() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .stopped)
    let chain = VMSnapshotChain(
      activeDisk: VMActiveDisk(
        source: "snapshot-overlay",
        snapshot: "before-upgrade",
        path: "disks/snapshots/before-upgrade.qcow2",
        format: "qcow2",
        exists: true,
        activatedAtUnix: 1_710_000_250
      ),
      disks: []
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      snapshotChainResult: .success(chain),
      snapshotChainDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let staleChain = Task { await model.loadSnapshotChain(for: virtualMachine) }
    for _ in 0..<100 where model.loadingSnapshotChainID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    await staleChain.value

    XCTAssertNil(model.snapshotChain(for: virtualMachine))
    XCTAssertNil(model.snapshotChainError(for: virtualMachine))
    XCTAssertNil(model.alertMessage)
    XCTAssertNil(model.loadingSnapshotChainID)
  }

  func testUpdateClientIgnoresStaleLifecyclePlan() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    let plan = LifecyclePlan(
      vm: "Dev VM",
      action: .suspend,
      currentState: .running,
      targetState: .suspended,
      backend: "apple-vz",
      metadataOnly: true,
      executable: true,
      qmpCommand: nil,
      socketPath: nil,
      socketAvailable: false,
      blockers: [],
      notes: []
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      lifecyclePlanResult: .success(plan),
      lifecyclePlanDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let stalePlan = Task { await model.loadLifecyclePlan(action: .suspend, for: virtualMachine) }
    for _ in 0..<100 where model.loadingLifecyclePlanID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    await stalePlan.value

    XCTAssertNil(model.lifecyclePlan(for: virtualMachine))
    XCTAssertNil(model.lifecyclePlanError(for: virtualMachine))
    XCTAssertNil(model.alertMessage)
    XCTAssertNil(model.loadingLifecyclePlanID)
  }

  func testUpdateClientIgnoresStaleOpenPortPlan() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    let plan = OpenPortPlan(
      vm: "Dev VM",
      scheme: "http",
      host: "127.0.0.1",
      guestPort: 8080,
      hostPort: 18080,
      url: "http://127.0.0.1:18080",
      command: ["open", "http://127.0.0.1:18080"]
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      openPortPlanResult: .success(plan),
      openPortPlanDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let stalePlan = Task {
      await model.loadOpenPortPlan(guestPort: "8080", scheme: "http", for: virtualMachine)
    }
    for _ in 0..<100 where model.loadingOpenPortPlanID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    let didLoad = await stalePlan.value

    XCTAssertFalse(didLoad)
    XCTAssertNil(model.openPortPlan(for: virtualMachine))
    XCTAssertNil(model.openPortPlanError(for: virtualMachine))
    XCTAssertNil(model.alertMessage)
    XCTAssertNil(model.loadingOpenPortPlanID)
  }

  func testUpdateClientIgnoresStaleSSHPlan() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    let plan = SSHPlan(
      vm: "Dev VM",
      user: "ubuntu",
      host: "127.0.0.1",
      port: 2222,
      source: .portForward,
      command: ["ssh", "-p", "2222", "ubuntu@127.0.0.1"]
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      sshPlanResult: .success(plan),
      sshPlanDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let stalePlan = Task { await model.loadSSHPlan(user: "ubuntu", for: virtualMachine) }
    for _ in 0..<100 where model.loadingSSHPlanID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    let didLoad = await stalePlan.value

    XCTAssertFalse(didLoad)
    XCTAssertNil(model.sshPlan(for: virtualMachine))
    XCTAssertNil(model.sshPlanError(for: virtualMachine))
    XCTAssertNil(model.alertMessage)
    XCTAssertNil(model.loadingSSHPlanID)
  }

  func testUpdateClientIgnoresStaleNetworkPlanAndPortForwards() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    let networkPlan = NetworkPlan(
      vm: "Dev VM",
      backend: "qemu",
      mode: "nat",
      hostname: "dev-vm",
      dryRun: true,
      executable: true,
      portForwards: [VMPortForward(host: 18080, guest: 8080)],
      capabilities: nil,
      blockers: [],
      notes: []
    )
    let forwards = VMPortForwardList(
      vm: "Dev VM",
      forwards: [VMPortForward(host: 18080, guest: 8080)]
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      networkPlanResult: .success(networkPlan),
      portForwardListResult: .success(forwards),
      networkPlanDelayNanos: 80_000_000,
      portForwardListDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let staleNetwork = Task { await model.loadNetworkPlan(for: virtualMachine) }
    let staleForwards = Task { await model.loadPortForwards(for: virtualMachine) }
    for _ in 0..<100
    where model.loadingNetworkPlanID == nil || model.loadingPortForwardsID == nil
    {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    await staleNetwork.value
    await staleForwards.value

    XCTAssertNil(model.networkPlan(for: virtualMachine))
    XCTAssertNil(model.networkPlanError(for: virtualMachine))
    XCTAssertNil(model.portForwardList(for: virtualMachine))
    XCTAssertNil(model.portForwardError(for: virtualMachine))
    XCTAssertNil(model.alertMessage)
    XCTAssertNil(model.loadingNetworkPlanID)
    XCTAssertNil(model.loadingPortForwardsID)
  }

  func testUpdateClientIgnoresStaleSharedFolders() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let shares = VMSharedFolderList(
      vm: "Dev VM",
      sharedFolders: [
        VMSharedFolder(
          name: "stale",
          hostPath: "/Users/dev/stale",
          readOnly: false,
          hostPathToken: "stale-token"
        )
      ]
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      sharedFolderListResult: .success(shares),
      sharedFolderListDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let staleSharedFolders = Task { await model.loadSharedFolders(for: virtualMachine) }
    for _ in 0..<100 where model.loadingSharedFoldersID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    await staleSharedFolders.value

    XCTAssertNil(model.sharedFolderList(for: virtualMachine))
    XCTAssertNil(model.sharedFolderError(for: virtualMachine))
    XCTAssertNil(model.alertMessage)
    XCTAssertNil(model.loadingSharedFoldersID)
  }

  func testUpdateClientIgnoresStalePrimaryDiskInspection() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let preparation = DiskPreparation(
      path: "disks/root.qcow2",
      format: "qcow2",
      size: "64G",
      sizeBytes: 68_719_476_736,
      exists: true,
      created: false,
      createCommand: nil,
      preparedAtUnix: 1_710_000_100
    )
    let inspection = VMDiskInspection(
      preparation: preparation,
      command: ["qemu-img", "info", "--output=json", "disks/root.qcow2"],
      exitStatus: "exit status: 0",
      info: "{\n  \"format\" : \"qcow2\"\n}",
      infoValue: .object(["format": .string("qcow2")]),
      stdout: "{\"format\":\"qcow2\"}",
      stderr: "",
      inspectDurationMicroseconds: 64,
      inspectedAtUnix: 1_710_000_300
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      diskInspectionResult: .success(inspection),
      diskInspectionDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let staleInspection = Task { await model.inspectPrimaryDisk(for: virtualMachine) }
    for _ in 0..<100 where model.inspectingDiskID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    let inspected = await staleInspection.value

    XCTAssertFalse(inspected)
    XCTAssertNil(model.diskInspection(for: virtualMachine))
    XCTAssertNil(model.diskPreparation(for: virtualMachine))
    XCTAssertNil(model.diskInspectionError(for: virtualMachine))
    XCTAssertNil(model.alertMessage)
    XCTAssertNil(model.inspectingDiskID)
  }

  func testUpdateClientIgnoresStalePrimaryDiskPreparation() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let preparation = DiskPreparation(
      path: "disks/root.qcow2",
      format: "qcow2",
      size: "64G",
      sizeBytes: 68_719_476_736,
      exists: true,
      created: false,
      createCommand: nil,
      preparedAtUnix: 1_710_000_100
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      diskPreparationResult: .success(preparation),
      diskPreparationDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let stalePreparation = Task { await model.preparePrimaryDisk(for: virtualMachine) }
    for _ in 0..<100 where model.preparingDiskID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    let prepared = await stalePreparation.value

    XCTAssertFalse(prepared)
    XCTAssertNil(model.diskPreparation(for: virtualMachine))
    XCTAssertNil(model.diskPreparationError(for: virtualMachine))
    XCTAssertNil(model.alertMessage)
    XCTAssertNil(model.preparingDiskID)
  }

  func testUpdateClientIgnoresStalePrimaryDiskCreation() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let preparation = DiskPreparation(
      path: "disks/root.qcow2",
      format: "qcow2",
      size: "64G",
      sizeBytes: 68_719_476_736,
      exists: true,
      created: true,
      createCommand: ["qemu-img", "create", "-f", "qcow2", "disks/root.qcow2", "64G"],
      preparedAtUnix: 1_710_000_100
    )
    let creation = VMDiskCreation(
      preparation: preparation,
      command: ["qemu-img", "create", "-f", "qcow2", "disks/root.qcow2", "64G"],
      executed: true,
      exitStatus: "exit status: 0",
      stdout: "",
      stderr: "",
      createdAtUnix: 1_710_000_200
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      diskCreationResult: .success(creation),
      diskCreationDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let staleCreation = Task { await model.createPrimaryDisk(for: virtualMachine) }
    for _ in 0..<100 where model.creatingDiskID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    let created = await staleCreation.value

    XCTAssertFalse(created)
    XCTAssertNil(model.diskCreation(for: virtualMachine))
    XCTAssertNil(model.diskPreparation(for: virtualMachine))
    XCTAssertNil(model.diskCreationError(for: virtualMachine))
    XCTAssertNil(model.alertMessage)
    XCTAssertNil(model.creatingDiskID)
  }

  func testUpdateClientIgnoresStaleActiveDiskVerification() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let verification = VMDiskVerification(
      activeDisk: VMActiveDisk(
        source: "primary",
        snapshot: nil,
        path: "disks/root.qcow2",
        format: "qcow2",
        exists: true,
        activatedAtUnix: 1_710_000_100
      ),
      command: ["qemu-img", "check", "--output=json", "disks/root.qcow2"],
      exitStatus: "exit status: 0",
      report: "{\n  \"check-errors\" : 0\n}",
      reportValue: .object(["check-errors": .int(0)]),
      stdout: "{\"check-errors\":0}",
      stderr: "",
      verifyDurationMicroseconds: 42,
      verifiedAtUnix: 1_710_000_400
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      diskVerificationResult: .success(verification),
      diskVerificationDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let staleVerification = Task { await model.verifyActiveDisk(for: virtualMachine) }
    for _ in 0..<100 where model.verifyingDiskID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    let verified = await staleVerification.value

    XCTAssertFalse(verified)
    XCTAssertNil(model.diskVerification(for: virtualMachine))
    XCTAssertNil(model.diskVerificationError(for: virtualMachine))
    XCTAssertNil(model.alertMessage)
    XCTAssertNil(model.verifyingDiskID)
  }

  func testUpdateClientIgnoresStaleActiveDiskCompaction() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let activeDisk = VMActiveDisk(
      source: "primary",
      snapshot: nil,
      path: "disks/root.qcow2",
      format: "qcow2",
      exists: true,
      activatedAtUnix: 1_710_000_100
    )
    let compaction = VMDiskCompaction(
      preparation: DiskPreparation(
        path: "disks/root.qcow2",
        format: "qcow2",
        size: "64G",
        sizeBytes: 1_024,
        exists: true,
        created: false,
        createCommand: nil,
        preparedAtUnix: 1_710_000_100
      ),
      activeDisk: activeDisk,
      command: ["qemu-img", "convert", "-O", "qcow2", "disks/root.qcow2"],
      tempPath: "disks/root.compact.tmp",
      backupPath: "disks/root.precompact-1710000500.qcow2",
      exitStatus: "exit status: 0",
      stdout: "",
      stderr: "",
      originalSizeBytes: 1_024,
      compactedSizeBytes: 512,
      compactDurationMicroseconds: 84,
      compactedAtUnix: 1_710_000_500
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      diskCompactionResult: .success(compaction),
      diskCompactionDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let staleCompaction = Task { await model.compactActiveDisk(for: virtualMachine) }
    for _ in 0..<100 where model.compactingDiskID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    let compacted = await staleCompaction.value

    XCTAssertFalse(compacted)
    XCTAssertNil(model.diskCompaction(for: virtualMachine))
    XCTAssertNil(model.diskCompactionError(for: virtualMachine))
    XCTAssertNil(model.snapshotChain(for: virtualMachine))
    XCTAssertTrue(oldClient.inspectedSnapshotChainIDs.isEmpty)
    XCTAssertNil(model.alertMessage)
    XCTAssertNil(model.compactingDiskID)
  }

  func testUpdateClientIgnoresStaleLogView() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let log = VMLogView(
      vm: "Dev VM",
      kind: .qemu,
      path: "/tmp/dev.vmbridge/logs/qemu.log",
      exists: true,
      bytes: 128,
      returnedBytes: 32,
      truncated: true,
      content: "stale qemu tail"
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      logViewResult: .success(log),
      logViewDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let staleLog = Task { await model.loadLogView(kind: .qemu, for: virtualMachine) }
    for _ in 0..<100 where model.loadingLogViewID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    await staleLog.value

    XCTAssertNil(model.logView(kind: .qemu, for: virtualMachine))
    XCTAssertNil(model.logViewError(for: virtualMachine))
    XCTAssertNil(model.alertMessage)
    XCTAssertNil(model.loadingLogViewID)
  }

  func testUpdateClientIgnoresStaleConsoleViewerEndpoint() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu x86_64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let plan = QemuLaunchPlan(
      program: "qemu-system-x86_64",
      args: ["-name", "Dev VM", "-display", "vnc=:0"]
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      qemuLaunchPlanResult: .success(plan),
      qemuLaunchPlanDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    var openedURLs: [URL] = []
    let model = DashboardViewModel(
      client: oldClient,
      openExternalURL: { url in
        openedURLs.append(url)
        return true
      }
    )

    await model.load()
    let staleOpen = Task { await model.openConsole(for: virtualMachine) }
    for _ in 0..<100 where oldClient.inspectedQemuLaunchPlanIDs.isEmpty {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    let didOpen = await staleOpen.value

    XCTAssertFalse(didOpen)
    XCTAssertNil(model.qemuLaunchPlan(for: virtualMachine))
    XCTAssertNil(model.qemuLaunchPlanError(for: virtualMachine))
    XCTAssertTrue(openedURLs.isEmpty)
    XCTAssertTrue(oldClient.inspectedQMPStatusIDs.isEmpty)
    XCTAssertNil(model.alertMessage)
  }

  func testCreateVirtualMachineLoadsTemplatesCreatesAndReloadsInventory() async throws {
    let template = BootTemplate(
      id: "ubuntu-arm64-installer",
      guestOS: "ubuntu",
      guestVersion: nil,
      guestArch: "arm64",
      mode: .linuxInstaller,
      mediaLabel: "ubuntu arm64 installer image",
      source: "manual",
      installerImage: "installers/ubuntu-arm64.iso",
      kernelPath: nil,
      initrdPath: nil,
      kernelCommandLine: nil,
      macosRestoreImage: nil,
      note: "Place the installer image inside the bundle."
    )
    let created = VirtualMachine(
      id: UUID(),
      name: "Created VM",
      guest: "Ubuntu ARM64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 0, memoryGB: 0, diskGB: 80),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([]),
      templatesResult: .success([template]),
      createResult: .success(created)
    )
    let model = DashboardViewModel(client: client)

    await model.loadBootTemplates()
    client.listResult = .success([created])
    let didCreate = await model.createVirtualMachine(
      name: "  Created VM  ",
      templateID: template.id
    )

    XCTAssertTrue(didCreate)
    XCTAssertEqual(
      client.createdRequests, [CreateVirtualMachineRequest(name: "Created VM", template: template)])
    XCTAssertEqual(model.virtualMachines, [created])
    XCTAssertEqual(model.selection, created.id)
    XCTAssertEqual(model.alertMessage, "Created VM created.")
  }

  func testLoadModeRecommendationRequestsGuestChoiceAndStoresGuidance() async throws {
    let template = BootTemplate(
      id: "windows-arm64",
      guestOS: "windows",
      guestVersion: "11",
      guestArch: "arm64",
      mode: .existingDisk,
      mediaLabel: "Windows Arm disk",
      source: "manual",
      installerImage: nil,
      kernelPath: nil,
      initrdPath: nil,
      kernelCommandLine: nil,
      macosRestoreImage: nil,
      note: "Provide an existing disk."
    )
    let recommendation = ModeRecommendation(
      mode: .compatibility,
      performance: "Medium; restricted QEMU/HVF path today",
      batteryImpact: "Higher than Apple VZ Fast Mode",
      integration: "Windows beta; not Apple VZ Fast Mode",
      message:
        "Windows 11 Arm uses Compatibility Mode with a restricted QEMU/HVF backend today. Apple VZ Fast Mode is Linux/macOS Arm only; BridgeVM must not claim Microsoft-authorized or Parallels-class Windows support.",
      fastModeAvailable: false,
      bootTemplate: nil
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([]),
      recommendationResult: .success(recommendation)
    )
    let model = DashboardViewModel(client: client)

    await model.loadModeRecommendation(for: template)

    XCTAssertEqual(client.requestedModeChoices, [GuestChoice(template: template)])
    XCTAssertEqual(model.modeRecommendation, recommendation)
    XCTAssertNil(model.modeRecommendationError)
    XCTAssertFalse(model.isLoadingModeRecommendation)
  }

  func testUpdateClientIgnoresStaleModeRecommendation() async throws {
    let template = BootTemplate(
      id: "windows-arm64",
      guestOS: "windows",
      guestVersion: "11",
      guestArch: "arm64",
      mode: .existingDisk,
      mediaLabel: "Windows Arm disk",
      source: "manual",
      installerImage: nil,
      kernelPath: nil,
      initrdPath: nil,
      kernelCommandLine: nil,
      macosRestoreImage: nil,
      note: "Provide an existing disk."
    )
    let recommendation = ModeRecommendation(
      mode: .compatibility,
      performance: "High for productivity workloads",
      batteryImpact: "Low to medium",
      integration: "Experimental",
      message: "Stale recommendation",
      fastModeAvailable: false,
      bootTemplate: nil
    )
    let staleClient = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([]),
      recommendationResult: .success(recommendation),
      recommendationDelayNanos: 50_000_000
    )
    let model = DashboardViewModel(client: staleClient)

    let staleRecommendation = Task { await model.loadModeRecommendation(for: template) }
    try await Task.sleep(nanoseconds: 10_000_000)
    model.updateClient(
      StubVirtualMachineClient(
        sourceTitle: "New inventory",
        listResult: .success([])
      )
    )

    await staleRecommendation.value

    XCTAssertEqual(staleClient.requestedModeChoices, [GuestChoice(template: template)])
    XCTAssertNil(model.modeRecommendation)
    XCTAssertNil(model.modeRecommendationError)
    XCTAssertFalse(model.isLoadingModeRecommendation)
  }

  func testCloneVirtualMachineRejectsEmptyName() async throws {
    let source = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([source])
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didClone = await model.cloneVirtualMachine(name: "   ", linked: false, for: source)

    XCTAssertFalse(didClone)
    XCTAssertTrue(client.clonedRequests.isEmpty)
    XCTAssertNil(model.cloningVirtualMachineID)
    XCTAssertEqual(model.alertMessage, "Enter a clone name.")
  }

  func testCloneVirtualMachineReloadsInventorySelectsCloneAndShowsAlert() async throws {
    let source = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "source"
    )
    let clone = VirtualMachine(
      id: UUID(),
      name: "Dev VM Copy",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "Cloned from Dev VM."
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([source]),
      cloneResult: .success(
        CloneVirtualMachineMetadata(
          vm: clone.name,
          source: "/Mock/Dev VM.vmbridge",
          output: "/Mock/Dev VM Copy.vmbridge"
        )
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    client.listResult = .success([clone, source])
    let didClone = await model.cloneVirtualMachine(
      name: "  Dev VM Copy  ",
      linked: false,
      for: source
    )

    XCTAssertTrue(didClone)
    XCTAssertEqual(client.clonedRequests.count, 1)
    XCTAssertEqual(client.clonedRequests[0].id, source.id)
    XCTAssertEqual(client.clonedRequests[0].newName, "Dev VM Copy")
    XCTAssertFalse(client.clonedRequests[0].linked)
    XCTAssertEqual(model.virtualMachines, [clone, source])
    XCTAssertEqual(model.selection, clone.id)
    XCTAssertNil(model.cloningVirtualMachineID)
    XCTAssertEqual(model.alertMessage, "Dev VM Copy cloned from Dev VM.")
  }

  func testCloneVirtualMachinePassesLinkedCloneFlagAndKeepsMetadataFlow() async throws {
    let source = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "source"
    )
    let clone = VirtualMachine(
      id: UUID(),
      name: "Dev VM Linked",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "Linked clone from Dev VM."
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([source]),
      cloneResult: .success(
        CloneVirtualMachineMetadata(
          vm: clone.name,
          source: "/Mock/Dev VM.vmbridge",
          output: "/Mock/Dev VM Linked.vmbridge"
        )
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    client.listResult = .success([source, clone])
    let didClone = await model.cloneVirtualMachine(
      name: " Dev VM Linked ",
      linked: true,
      for: source
    )

    XCTAssertTrue(didClone)
    XCTAssertEqual(client.clonedRequests.count, 1)
    XCTAssertEqual(client.clonedRequests[0].id, source.id)
    XCTAssertEqual(client.clonedRequests[0].newName, "Dev VM Linked")
    XCTAssertTrue(client.clonedRequests[0].linked)
    XCTAssertEqual(model.virtualMachines, [source, clone])
    XCTAssertEqual(model.selection, clone.id)
    XCTAssertNil(model.cloningVirtualMachineID)
    XCTAssertEqual(model.alertMessage, "Dev VM Linked cloned from Dev VM.")
  }

  func testDeleteVirtualMachineRejectsRunningVMBeforeClientRequest() async throws {
    let running = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([running])
    )
    let model = DashboardViewModel(client: client)

    let didDelete = await model.deleteVirtualMachine(running)

    XCTAssertFalse(didDelete)
    XCTAssertTrue(client.deletedVMIDs.isEmpty)
    XCTAssertNil(model.deletingVirtualMachineID)
    XCTAssertEqual(model.alertMessage, "Stop Dev VM before deleting it.")
  }

  func testDeleteVirtualMachineUsesMetadataOnlyDeleteReloadsInventoryAndClearsSelection()
    async throws
  {
    let deleted = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let remaining = VirtualMachine(
      id: UUID(),
      name: "Other VM",
      guest: "Debian x86_64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 2, memoryGB: 4, diskGB: 40),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let metadata = VMDeletionMetadata(
      path: "/Mock/Dev VM.vmbridge",
      metadataOnly: true,
      vm: deleted.name
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([deleted, remaining]),
      deleteResult: .success(metadata)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    client.listResult = .success([remaining])
    let didDelete = await model.deleteVirtualMachine(deleted)

    XCTAssertTrue(didDelete)
    XCTAssertEqual(client.deletedVMIDs, [deleted.id])
    XCTAssertEqual(model.virtualMachines, [remaining])
    XCTAssertNil(model.selection)
    XCTAssertNil(model.selectedVirtualMachine)
    XCTAssertNil(model.deletingVirtualMachineID)
    XCTAssertEqual(model.alertMessage, "Dev VM deleted from metadata at /Mock/Dev VM.vmbridge.")
  }

  func testExportVirtualMachineRejectsEmptyOutputBeforeClientRequest() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: client)

    let didExport = await model.exportVirtualMachine(output: "   ", for: virtualMachine)

    XCTAssertFalse(didExport)
    XCTAssertTrue(client.exportedVMRequests.isEmpty)
    XCTAssertNil(model.exportingVirtualMachineID)
    XCTAssertEqual(model.alertMessage, "Enter an export output path.")
  }

  func testExportVirtualMachineTrimsOutputStoresMetadataClearsBusyFlagAndShowsAlert()
    async throws
  {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let metadata = VMExportMetadata(
      vm: "Dev VM",
      source: "/Mock/Dev VM.vmbridge",
      output: "/tmp/Dev VM.vmbridge",
      archiveFormat: "directory",
      copiedFileCount: 3,
      copiedFiles: ["manifest.yaml", "metadata/state.json", "metadata/runtime.json"],
      manifestPreserved: true,
      metadataPreserved: true,
      exportedAtUnix: 1_710_000_040
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      exportResult: .success(metadata)
    )
    let model = DashboardViewModel(client: client)

    let didExport = await model.exportVirtualMachine(
      output: "  /tmp/Dev VM.vmbridge  ",
      for: virtualMachine
    )

    XCTAssertTrue(didExport)
    XCTAssertEqual(client.exportedVMRequests.count, 1)
    XCTAssertEqual(client.exportedVMRequests[0].id, virtualMachine.id)
    XCTAssertEqual(client.exportedVMRequests[0].output, "/tmp/Dev VM.vmbridge")
    XCTAssertEqual(model.vmExport(for: virtualMachine), metadata)
    XCTAssertEqual(model.vmExport(for: virtualMachine)?.archiveFormat, "directory")
    XCTAssertEqual(model.vmExport(for: virtualMachine)?.copiedFileCount, 3)
    XCTAssertEqual(
      model.vmExport(for: virtualMachine)?.copiedFiles,
      [
        "manifest.yaml",
        "metadata/state.json",
        "metadata/runtime.json",
      ])
    XCTAssertEqual(model.vmExport(for: virtualMachine)?.manifestPreserved, true)
    XCTAssertEqual(model.vmExport(for: virtualMachine)?.metadataPreserved, true)
    XCTAssertNil(model.vmExportError(for: virtualMachine))
    XCTAssertNil(model.exportingVirtualMachineID)
    XCTAssertEqual(model.alertMessage, "Dev VM exported to /tmp/Dev VM.vmbridge.")
  }

  func testImportVirtualMachineRejectsEmptyInputBeforeClientRequest() async throws {
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([])
    )
    let model = DashboardViewModel(client: client)

    let didImport = await model.importVirtualMachine(input: "   ", name: "Imported VM")

    XCTAssertFalse(didImport)
    XCTAssertTrue(client.importedVMRequests.isEmpty)
    XCTAssertFalse(model.isImportingVirtualMachine)
    XCTAssertEqual(model.alertMessage, "Enter an import input path.")
  }

  func
    testImportVirtualMachineTrimsInputAndNameStoresLastImportReloadsInventorySelectsVMAndShowsAlert()
    async throws
  {
    let imported = VirtualMachine(
      id: UUID(),
      name: "Imported VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "imported"
    )
    let metadata = VMImportMetadata(
      vm: imported.name,
      source: "/tmp/source.vmbridge",
      output: "/Mock/Imported VM.vmbridge",
      archiveFormat: "directory",
      copiedFileCount: 3,
      copiedFiles: ["manifest.yaml", "metadata/state.json", "metadata/runtime.json"],
      manifestPreserved: true,
      metadataPreserved: true,
      originalName: "source",
      requestedName: "Imported VM",
      manifestIdentityRewritten: true,
      importedAtUnix: 1_710_000_040
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([]),
      vmImportResult: .success(metadata)
    )
    let model = DashboardViewModel(client: client)

    client.listResult = .success([imported])
    let didImport = await model.importVirtualMachine(
      input: "  /tmp/source.vmbridge  ",
      name: "  Imported VM  "
    )

    XCTAssertTrue(didImport)
    XCTAssertEqual(client.importedVMRequests.count, 1)
    XCTAssertEqual(client.importedVMRequests[0].input, "/tmp/source.vmbridge")
    XCTAssertEqual(client.importedVMRequests[0].name, "Imported VM")
    XCTAssertEqual(model.lastVMImport, metadata)
    XCTAssertEqual(model.lastVMImport?.archiveFormat, "directory")
    XCTAssertEqual(model.lastVMImport?.copiedFileCount, 3)
    XCTAssertEqual(
      model.lastVMImport?.copiedFiles,
      [
        "manifest.yaml",
        "metadata/state.json",
        "metadata/runtime.json",
      ])
    XCTAssertEqual(model.lastVMImport?.manifestPreserved, true)
    XCTAssertEqual(model.lastVMImport?.metadataPreserved, true)
    XCTAssertEqual(model.lastVMImport?.originalName, "source")
    XCTAssertEqual(model.lastVMImport?.requestedName, "Imported VM")
    XCTAssertEqual(model.lastVMImport?.manifestIdentityRewritten, true)
    XCTAssertNil(model.vmImportError)
    XCTAssertEqual(model.virtualMachines, [imported])
    XCTAssertEqual(model.selection, imported.id)
    XCTAssertFalse(model.isImportingVirtualMachine)
    XCTAssertEqual(model.alertMessage, "Imported VM imported from /tmp/source.vmbridge.")
  }

  func testLifecycleActionsReflectCurrentVmStatus() async throws {
    let running = VirtualMachine(
      id: UUID(),
      name: "Running VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    let suspended = VirtualMachine(
      id: UUID(),
      name: "Suspended VM",
      guest: "Ubuntu Arm64",
      status: .suspended,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Suspended",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let stopped = VirtualMachine(
      id: UUID(),
      name: "Stopped VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    var needsAttention = stopped
    needsAttention.status = .error
    needsAttention.uptime = "Launch failed"
    let model = DashboardViewModel(
      client: StubVirtualMachineClient(
        sourceTitle: "Mock inventory",
        listResult: .success([running, suspended, stopped, needsAttention])
      )
    )

    XCTAssertEqual(running.primaryActionTitle, "Suspend")
    let runningActions = model.lifecycleActions(for: running)
    XCTAssertEqual(runningActions.map(\.action), [.pause, .restart, .stop])
    XCTAssertEqual(runningActions.first?.title, "Suspend")
    XCTAssertEqual(
      runningActions.first?.detail,
      "Suspend this VM and save its machine state to disk."
    )
    XCTAssertEqual(model.lifecycleActions(for: suspended).map(\.action), [.resume, .stop])
    XCTAssertEqual(suspended.primaryActionTitle, "Resume")
    let stoppedActions = model.lifecycleActions(for: stopped)
    XCTAssertEqual(stopped.primaryActionTitle, "Start")
    XCTAssertEqual(stoppedActions.map(\.action), [.start])
    XCTAssertEqual(
      stoppedActions.first?.detail,
      "Prepare launch readiness, then ask the daemon to launch the backend when ready."
    )
    let errorActions = model.lifecycleActions(for: needsAttention)
    XCTAssertEqual(errorActions.map(\.action), [.start, .stop])
    XCTAssertEqual(
      errorActions.first?.detail,
      "Retry launch readiness, then ask the daemon to launch the backend when ready."
    )
    XCTAssertTrue(model.lifecycleActions(for: running).contains { $0.isDestructive })
  }

  func testPerformLifecycleActionRecordsClientActionAndUpdatesInventory() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    var suspended = virtualMachine
    suspended.status = .suspended
    suspended.uptime = "Suspended"
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      performResult: .success(
        VMActionResult(
          virtualMachine: suspended,
          message: "Dev VM suspended."
        )
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.perform(.pause, on: virtualMachine)

    XCTAssertEqual(client.performedActionRequests.count, 1)
    XCTAssertEqual(client.performedActionRequests[0].action, .pause)
    XCTAssertEqual(client.performedActionRequests[0].id, virtualMachine.id)
    XCTAssertEqual(model.virtualMachines, [suspended])
    XCTAssertEqual(model.selection, suspended.id)
    XCTAssertNil(model.activeActionID)
    XCTAssertEqual(model.alertMessage, "Dev VM suspended.")
  }

  func testPerformLifecycleActionBlocksWhenInventoryComesFromFallback() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Fallback VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      allowsMutationsForCurrentInventory: false,
      listResult: .success([virtualMachine]),
      performResult: .success(
        VMActionResult(
          virtualMachine: virtualMachine,
          message: "should not mutate"
        )
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.perform(.pause, on: virtualMachine)

    XCTAssertTrue(client.performedActionRequests.isEmpty)
    XCTAssertNil(model.activeActionID)
    XCTAssertEqual(model.virtualMachines, [virtualMachine])
    XCTAssertEqual(
      model.alertMessage,
      "Suspended blocked for Fallback VM: the current VM list came from fallback inventory. Refresh bridgevmd inventory or switch to mock inventory before changing this VM."
    )
  }

  func testPerformLifecycleActionIgnoresDuplicateWhileActionIsActive() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    var suspended = virtualMachine
    suspended.status = .suspended
    suspended.uptime = "Suspended"
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      performResult: .success(
        VMActionResult(
          virtualMachine: suspended,
          message: "Dev VM suspended."
        )
      ),
      performDelayNanos: 50_000_000
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let firstAction = Task { await model.perform(.pause, on: virtualMachine) }
    for _ in 0..<100 where model.activeActionID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    XCTAssertEqual(model.activeActionID, virtualMachine.id)
    await model.perform(.restart, on: virtualMachine)
    XCTAssertEqual(client.performedActionRequests.map(\.action), [.pause])

    await firstAction.value
    XCTAssertNil(model.activeActionID)
  }

  func testUpdateClientIgnoresStaleLifecycleActionResult() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "old"
    )
    var suspended = virtualMachine
    suspended.status = .suspended
    suspended.uptime = "Suspended"
    suspended.notes = "stale"
    var replacement = virtualMachine
    replacement.notes = "new"
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      performResult: .success(
        VMActionResult(
          virtualMachine: suspended,
          message: "Dev VM suspended."
        )
      ),
      performDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([replacement])
    )
    let model = DashboardViewModel(client: oldClient)

    await model.load()
    let staleAction = Task { await model.perform(.pause, on: virtualMachine) }
    for _ in 0..<100 where model.activeActionID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    XCTAssertEqual(model.activeActionID, virtualMachine.id)
    model.updateClient(newClient)
    XCTAssertNil(model.activeActionID)
    await model.load()
    await staleAction.value

    XCTAssertEqual(model.virtualMachines, [replacement])
    XCTAssertEqual(model.selection, replacement.id)
    XCTAssertNil(model.alertMessage)
    XCTAssertEqual(oldClient.performedActionRequests.map(\.action), [.pause])
  }

  func testPerformPrimaryActionPreparesRunInsteadOfStartingStoppedOrErrorVMWhenLaunchReadinessBlocked()
    async throws
  {
    for status in [VirtualMachine.Status.stopped, .error] {
      let virtualMachine = primaryActionPreflightVirtualMachine(status: status)
      let runnerStatus = primaryActionPreflightRunnerStatus(
        launchReadiness: LaunchReadiness(
          ready: false,
          blockers: [
            LaunchReadinessBlocker(
              code: "missing-primary-disk",
              message: "Primary disk is missing.",
              path: "disks/root.qcow2",
              capability: "apple-vz"
            )
          ]
        )
      )
      let client = StubVirtualMachineClient(
        sourceTitle: "Mock inventory",
        listResult: .success([virtualMachine]),
        readinessReportResult: .success(
          primaryActionPreflightReadinessReport(
            virtualMachine: virtualMachine,
            runnerStatus: runnerStatus
          )
        ),
        prepareRunResult: .success(runnerStatus),
        runnerStatusResult: .success(runnerStatus),
        diskPreparationResult: .success(primaryActionPreflightDiskPreparation())
      )
      let model = DashboardViewModel(client: client)

      await model.load()
      await model.loadReadinessReport(for: virtualMachine)
      _ = await model.preparePrimaryDisk(for: virtualMachine)
      await model.performPrimaryAction(on: virtualMachine)

      XCTAssertTrue(client.performedActionRequests.isEmpty)
      XCTAssertEqual(client.preparedRunIDs, [virtualMachine.id])
    }
  }

  func testPerformPrimaryActionPreparesRunInsteadOfStartingStoppedOrErrorVMWhenLaunchReadinessMissing()
    async throws
  {
    for status in [VirtualMachine.Status.stopped, .error] {
      let virtualMachine = primaryActionPreflightVirtualMachine(status: status)
      let runnerStatus = primaryActionPreflightRunnerStatus(launchReadiness: nil)
      let client = StubVirtualMachineClient(
        sourceTitle: "Mock inventory",
        listResult: .success([virtualMachine]),
        readinessReportResult: .success(
          primaryActionPreflightReadinessReport(
            virtualMachine: virtualMachine,
            runnerStatus: runnerStatus
          )
        ),
        prepareRunResult: .success(runnerStatus),
        runnerStatusResult: .success(runnerStatus),
        diskPreparationResult: .success(primaryActionPreflightDiskPreparation())
      )
      let model = DashboardViewModel(client: client)

      await model.load()
      await model.loadReadinessReport(for: virtualMachine)
      _ = await model.preparePrimaryDisk(for: virtualMachine)
      await model.performPrimaryAction(on: virtualMachine)

      XCTAssertTrue(client.performedActionRequests.isEmpty)
      XCTAssertEqual(client.preparedRunIDs, [virtualMachine.id])
    }
  }

  func testPerformPrimaryActionStartsStoppedOrErrorVMWhenLaunchReadinessReady() async throws {
    for status in [VirtualMachine.Status.stopped, .error] {
      let virtualMachine = primaryActionPreflightVirtualMachine(status: status)
      var started = virtualMachine
      started.status = .running
      started.uptime = "Metadata start recorded"
      let runnerStatus = primaryActionPreflightRunnerStatus(
        launchReadiness: LaunchReadiness(ready: true, blockers: [])
      )
      let client = StubVirtualMachineClient(
        sourceTitle: "Mock inventory",
        listResult: .success([virtualMachine]),
        readinessReportResult: .success(
          primaryActionPreflightReadinessReport(
            virtualMachine: virtualMachine,
            runnerStatus: runnerStatus
          )
        ),
        runnerStatusResult: .success(runnerStatus),
        performResult: .success(
          VMActionResult(
            virtualMachine: started,
            message: "\(virtualMachine.name) metadata start recorded."
          )
        )
      )
      let model = DashboardViewModel(client: client)

      await model.load()
      await model.loadReadinessReport(for: virtualMachine)
      await model.performPrimaryAction(on: virtualMachine)

      XCTAssertEqual(client.performedActionRequests.map(\.action), [.start])
      XCTAssertEqual(client.performedActionRequests.map(\.id), [virtualMachine.id])
      XCTAssertTrue(client.preparedRunIDs.isEmpty)
    }
  }

  func testInventoryRefreshClearsReadinessCachesWhenVmMetadataChanges() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .stopped)
    var changedVirtualMachine = virtualMachine
    changedVirtualMachine.notes = "metadata changed"
    let runnerStatus = primaryActionPreflightRunnerStatus(
      launchReadiness: LaunchReadiness(ready: true, blockers: [])
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      readinessReportResult: .success(
        primaryActionPreflightReadinessReport(
          virtualMachine: virtualMachine,
          runnerStatus: runnerStatus
        )
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.loadReadinessReport(for: virtualMachine)
    XCTAssertEqual(model.runnerStatus(for: virtualMachine), runnerStatus)
    XCTAssertNotNil(model.readinessReport(for: virtualMachine))
    XCTAssertNotNil(model.bootMediaStatus(for: virtualMachine))
    XCTAssertNotNil(model.snapshotChain(for: virtualMachine))

    client.listResult = .success([changedVirtualMachine])
    await model.load()

    XCTAssertEqual(model.virtualMachines, [changedVirtualMachine])
    XCTAssertNil(model.runnerStatus(for: changedVirtualMachine))
    XCTAssertNil(model.readinessReport(for: changedVirtualMachine))
    XCTAssertNil(model.bootMediaStatus(for: changedVirtualMachine))
    XCTAssertNil(model.snapshotChain(for: changedVirtualMachine))
  }

  func testInventoryRefreshClearsReadinessCachesWhenVmStatusChanges() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .stopped)
    var runningVirtualMachine = virtualMachine
    runningVirtualMachine.status = .running
    runningVirtualMachine.uptime = "12m"
    let runnerStatus = primaryActionPreflightRunnerStatus(
      launchReadiness: LaunchReadiness(ready: true, blockers: [])
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      readinessReportResult: .success(
        primaryActionPreflightReadinessReport(
          virtualMachine: virtualMachine,
          runnerStatus: runnerStatus
        )
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.loadReadinessReport(for: virtualMachine)
    XCTAssertEqual(model.runnerStatus(for: virtualMachine), runnerStatus)
    XCTAssertNotNil(model.readinessReport(for: virtualMachine))
    XCTAssertNotNil(model.bootMediaStatus(for: virtualMachine))
    XCTAssertNotNil(model.snapshotChain(for: virtualMachine))

    client.listResult = .success([runningVirtualMachine])
    await model.load()

    XCTAssertEqual(model.virtualMachines, [runningVirtualMachine])
    XCTAssertNil(model.runnerStatus(for: runningVirtualMachine))
    XCTAssertNil(model.readinessReport(for: runningVirtualMachine))
    XCTAssertNil(model.bootMediaStatus(for: runningVirtualMachine))
    XCTAssertNil(model.snapshotChain(for: runningVirtualMachine))
  }

  func testInventoryRefreshClearsReadinessCachesWhenVmDisappears() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .stopped)
    let runnerStatus = primaryActionPreflightRunnerStatus(
      launchReadiness: LaunchReadiness(ready: true, blockers: [])
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      readinessReportResult: .success(
        primaryActionPreflightReadinessReport(
          virtualMachine: virtualMachine,
          runnerStatus: runnerStatus
        )
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.loadReadinessReport(for: virtualMachine)
    XCTAssertEqual(model.runnerStatus(for: virtualMachine), runnerStatus)
    XCTAssertNotNil(model.readinessReport(for: virtualMachine))
    XCTAssertNotNil(model.bootMediaStatus(for: virtualMachine))
    XCTAssertNotNil(model.snapshotChain(for: virtualMachine))

    client.listResult = .success([])
    await model.load()

    XCTAssertTrue(model.virtualMachines.isEmpty)
    XCTAssertNil(model.runnerStatus(for: virtualMachine))
    XCTAssertNil(model.readinessReport(for: virtualMachine))
    XCTAssertNil(model.bootMediaStatus(for: virtualMachine))
    XCTAssertNil(model.snapshotChain(for: virtualMachine))
  }

  func testLoadReadinessReportClearsStaleRunnerWhenReportOmitsRunner() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .stopped)
    let readyRunner = primaryActionPreflightRunnerStatus(
      launchReadiness: LaunchReadiness(ready: true, blockers: [])
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      readinessReportResult: .success(
        primaryActionPreflightReadinessReport(
          virtualMachine: virtualMachine,
          runnerStatus: readyRunner
        )
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.loadReadinessReport(for: virtualMachine)
    XCTAssertEqual(model.runnerStatus(for: virtualMachine), readyRunner)

    client.readinessReportResult = .success(
      primaryActionPreflightReadinessReport(
        virtualMachine: virtualMachine,
        runnerStatus: nil
      )
    )
    await model.loadReadinessReport(for: virtualMachine)

    XCTAssertNil(model.runnerStatus(for: virtualMachine))
    XCTAssertNil(model.runnerStatusError(for: virtualMachine))
    XCTAssertNotNil(model.readinessReport(for: virtualMachine))
  }

  func testPerformStartWarmsRunnerQemuAndGuestToolsStatusAfterBackendLaunch() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .stopped)
    var started = virtualMachine
    started.status = .running
    started.uptime = "Metadata start recorded"
    let runnerStatus = primaryActionPreflightRunnerStatus(
      launchReadiness: LaunchReadiness(ready: true, blockers: [])
    )
    let qemuLaunchPlan = QemuLaunchPlan(
      program: "qemu-system-x86_64",
      args: [
        "-name", virtualMachine.name,
        "-display", "vnc=:0",
      ]
    )
    let guestToolsStatus = Self.guestToolsStatus(vm: virtualMachine.name)
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      readinessReportResult: .success(
        primaryActionPreflightReadinessReport(
          virtualMachine: virtualMachine,
          runnerStatus: runnerStatus
        )
      ),
      guestToolsStatusResult: .success(guestToolsStatus),
      qemuLaunchPlanResult: .success(qemuLaunchPlan),
      runnerStatusResult: .success(runnerStatus),
      performResult: .success(
        VMActionResult(
          virtualMachine: started,
          message: "\(virtualMachine.name) metadata start recorded."
        )
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.loadReadinessReport(for: virtualMachine)
    await model.perform(.start, on: virtualMachine)

    XCTAssertEqual(client.performedActionRequests.map(\.action), [.start])
    XCTAssertEqual(client.performedActionRequests.map(\.id), [virtualMachine.id])
    XCTAssertEqual(client.inspectedRunnerStatusIDs, [virtualMachine.id])
    XCTAssertEqual(client.inspectedQemuLaunchPlanIDs, [virtualMachine.id])
    XCTAssertEqual(client.inspectedGuestToolsStatusIDs, [virtualMachine.id])
    XCTAssertEqual(model.virtualMachines, [started])
    XCTAssertEqual(model.runnerStatus(for: started), runnerStatus)
    XCTAssertNil(model.readinessReport(for: started))
    XCTAssertEqual(model.qemuLaunchPlan(for: started), qemuLaunchPlan)
    XCTAssertEqual(
      model.qemuLaunchPlan(for: started)?.viewerEndpoint?.absoluteString,
      "vnc://127.0.0.1:5900")
    XCTAssertEqual(model.guestToolsStatus(for: started), guestToolsStatus)
    XCTAssertNil(model.runnerStatusError(for: started))
    XCTAssertNil(model.qemuLaunchPlanError(for: started))
    XCTAssertNil(model.guestToolsStatusError(for: started))
    XCTAssertEqual(model.alertMessage, "\(virtualMachine.name) metadata start recorded.")
  }

  func testPerformRestartPreparesAndBlocksBeforeStoppingWhenLaunchReadinessFails() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    let blockedStatus = primaryActionPreflightRunnerStatus(
      launchReadiness: LaunchReadiness(
        ready: false,
        blockers: [
          LaunchReadinessBlocker(
            code: "missing-primary-disk",
            message: "Primary disk is missing.",
            path: "/tmp/root.raw",
            capability: "apple-vz"
          )
        ]
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      prepareRunResult: .success(blockedStatus)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.perform(.restart, on: virtualMachine)

    XCTAssertEqual(client.preparedRunIDs, [virtualMachine.id])
    XCTAssertTrue(client.performedActionRequests.isEmpty)
    XCTAssertEqual(model.runnerStatus(for: virtualMachine), blockedStatus)
    let expectedAlert =
      "\(virtualMachine.name) restart blocked: missing-primary-disk: Primary disk is missing. (/tmp/root.raw)."
    XCTAssertEqual(
      model.alertMessage,
      expectedAlert)
  }

  func testPerformRestartPreparesBeforeRestartingWhenLaunchReadinessIsReady() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    var restarted = virtualMachine
    restarted.uptime = "Metadata restart recorded"
    let runnerStatus = primaryActionPreflightRunnerStatus(
      launchReadiness: LaunchReadiness(ready: true, blockers: [])
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(Self.guestToolsStatus(vm: virtualMachine.name)),
      qemuLaunchPlanResult: .success(
        QemuLaunchPlan(program: "qemu-system-x86_64", args: ["-display", "vnc=:0"])
      ),
      prepareRunResult: .success(runnerStatus),
      runnerStatusResult: .success(runnerStatus),
      performResult: .success(
        VMActionResult(
          virtualMachine: restarted,
          message: "\(virtualMachine.name) restarted."
        )
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.perform(VirtualMachineAction.restart, on: virtualMachine)

    XCTAssertEqual(client.preparedRunIDs, [virtualMachine.id])
    XCTAssertEqual(
      client.performedActionRequests.map(\.action), [VirtualMachineAction.restart])
    XCTAssertEqual(
      client.performedActionRequests.map(\.id), [virtualMachine.id])
    XCTAssertEqual(model.virtualMachines, [restarted])
    XCTAssertEqual(model.alertMessage, "\(virtualMachine.name) restarted.")
  }

  func testPerformStopClearsRuntimeCachesAfterBackendStop() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    var stopped = virtualMachine
    stopped.status = .stopped
    stopped.uptime = "Not running"
    let runnerStatus = primaryActionPreflightRunnerStatus(
      launchReadiness: LaunchReadiness(ready: true, blockers: [])
    )
    let qemuLaunchPlan = QemuLaunchPlan(
      program: "qemu-system-x86_64",
      args: ["-display", "default,show-cursor=on"]
    )
    let qmpStatus = QMPStatus(
      socketPath: "/tmp/dev-qmp.sock",
      available: true,
      status: "running",
      running: true
    )
    let guestToolsStatus = Self.guestToolsStatus(vm: virtualMachine.name)
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      readinessReportResult: .success(
        primaryActionPreflightReadinessReport(
          virtualMachine: virtualMachine,
          runnerStatus: runnerStatus
        )
      ),
      guestToolsStatusResult: .success(guestToolsStatus),
      qmpStatusResult: .success(qmpStatus),
      qemuLaunchPlanResult: .success(qemuLaunchPlan),
      runnerStatusResult: .success(runnerStatus),
      performResult: .success(
        VMActionResult(
          virtualMachine: stopped,
          message: "\(virtualMachine.name) stopped."
        )
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.loadReadinessReport(for: virtualMachine)
    await model.loadRunnerStatus(for: virtualMachine)
    await model.loadQemuLaunchPlan(for: virtualMachine)
    await model.loadGuestToolsStatus(for: virtualMachine)
    _ = await model.openConsole(for: virtualMachine)
    await model.perform(.stop, on: virtualMachine)

    XCTAssertEqual(client.performedActionRequests.map(\.action), [.stop])
    XCTAssertEqual(model.runnerStatus(for: stopped), nil)
    XCTAssertEqual(model.readinessReport(for: stopped), nil)
    XCTAssertEqual(model.qemuLaunchPlan(for: stopped), nil)
    XCTAssertEqual(model.qmpStatus(for: stopped), nil)
    XCTAssertEqual(model.guestToolsStatus(for: stopped), nil)
    XCTAssertEqual(model.alertMessage, "\(virtualMachine.name) stopped.")
  }

  func testPerformPrimaryActionStartsAfterSuccessfulPrepareWhenPreRunReadinessIsReady()
    async throws
  {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .stopped)
    var started = virtualMachine
    started.status = .running
    started.uptime = "Metadata start recorded"
    let runnerStatus = primaryActionPreflightRunnerStatus(
      launchReadiness: LaunchReadiness(ready: true, blockers: [])
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      readinessReportResult: .success(
        primaryActionPreflightReadinessReport(
          virtualMachine: virtualMachine,
          runnerStatus: nil,
          preRunLaunchReadiness: LaunchReadiness(ready: true, blockers: [])
        )
      ),
      prepareRunResult: .success(runnerStatus),
      diskPreparationResult: .success(primaryActionPreflightDiskPreparation()),
      performResult: .success(
        VMActionResult(
          virtualMachine: started,
          message: "\(virtualMachine.name) metadata start recorded."
        )
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.loadReadinessReport(for: virtualMachine)
    _ = await model.preparePrimaryDisk(for: virtualMachine)
    await model.performPrimaryAction(on: virtualMachine)

    XCTAssertEqual(client.preparedRunIDs, [virtualMachine.id])
    XCTAssertEqual(client.performedActionRequests.map(\.action), [.start])
    XCTAssertEqual(client.performedActionRequests.map(\.id), [virtualMachine.id])
  }

  func testLoadLifecyclePlanStoresSelectedVmPlan() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    let plan = LifecyclePlan(
      vm: "Dev VM",
      action: .suspend,
      currentState: .running,
      targetState: .suspended,
      backend: "qemu-qmp",
      metadataOnly: true,
      executable: false,
      qmpCommand: "stop",
      socketPath: "/tmp/dev.vmbridge/run/qmp.sock",
      socketAvailable: false,
      blockers: ["qmp-socket-unavailable:/tmp/dev.vmbridge/run/qmp.sock"],
      notes: ["metadata-only lifecycle plan; no backend command was sent"]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      lifecyclePlanResult: .success(plan)
    )
    let model = DashboardViewModel(client: client)

    await model.loadLifecyclePlan(action: .suspend, for: virtualMachine)

    XCTAssertEqual(model.lifecyclePlan(for: virtualMachine), plan)
    XCTAssertNil(model.lifecyclePlanError(for: virtualMachine))
    XCTAssertEqual(client.inspectedLifecyclePlanRequests.count, 1)
    XCTAssertEqual(client.inspectedLifecyclePlanRequests[0].action, .suspend)
    XCTAssertEqual(client.inspectedLifecyclePlanRequests[0].id, virtualMachine.id)
    XCTAssertNil(model.loadingLifecyclePlanID)
  }

  func testLoadLifecyclePlanStoresError() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      lifecyclePlanResult: .failure(TestRefreshError.offline)
    )
    let model = DashboardViewModel(client: client)

    await model.loadLifecyclePlan(action: .resume, for: virtualMachine)

    XCTAssertNil(model.lifecyclePlan(for: virtualMachine))
    XCTAssertEqual(model.lifecyclePlanError(for: virtualMachine), "Offline")
    XCTAssertEqual(model.alertMessage, "Offline")
    XCTAssertEqual(client.inspectedLifecyclePlanRequests.map(\.action), [.resume])
    XCTAssertNil(model.loadingLifecyclePlanID)
  }

  func testLoadOpenPortPlanStoresSelectedVmPlan() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    let plan = OpenPortPlan(
      vm: "Dev VM",
      scheme: "https",
      host: "127.0.0.1",
      guestPort: 443,
      hostPort: 8443,
      url: "https://127.0.0.1:8443",
      command: ["open", "https://127.0.0.1:8443"]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      openPortPlanResult: .success(plan)
    )
    let model = DashboardViewModel(client: client)

    let loaded = await model.loadOpenPortPlan(
      guestPort: "443",
      scheme: "https",
      for: virtualMachine
    )

    XCTAssertTrue(loaded)
    XCTAssertEqual(model.openPortPlan(for: virtualMachine), plan)
    XCTAssertNil(model.openPortPlanError(for: virtualMachine))
    XCTAssertEqual(client.inspectedOpenPortPlanRequests.count, 1)
    XCTAssertEqual(client.inspectedOpenPortPlanRequests[0].guestPort, 443)
    XCTAssertEqual(client.inspectedOpenPortPlanRequests[0].scheme, "https")
    XCTAssertEqual(client.inspectedOpenPortPlanRequests[0].id, virtualMachine.id)
    XCTAssertNil(model.loadingOpenPortPlanID)
  }

  func testLoadOpenPortPlanStoresError() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      openPortPlanResult: .failure(TestRefreshError.offline)
    )
    let model = DashboardViewModel(client: client)

    let loaded = await model.loadOpenPortPlan(
      guestPort: "8080",
      scheme: "",
      for: virtualMachine
    )

    XCTAssertFalse(loaded)
    XCTAssertNil(model.openPortPlan(for: virtualMachine))
    XCTAssertEqual(model.openPortPlanError(for: virtualMachine), "Offline")
    XCTAssertEqual(model.alertMessage, "Offline")
    XCTAssertEqual(client.inspectedOpenPortPlanRequests.map(\.scheme), ["http"])
    XCTAssertNil(model.loadingOpenPortPlanID)
  }

  func testLoadOpenPortPlanRejectsInvalidGuestPort() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: client)

    let loaded = await model.loadOpenPortPlan(
      guestPort: "0",
      scheme: "http",
      for: virtualMachine
    )

    XCTAssertFalse(loaded)
    XCTAssertEqual(model.alertMessage, "Enter a valid guest port.")
    XCTAssertTrue(client.inspectedOpenPortPlanRequests.isEmpty)
    XCTAssertNil(model.loadingOpenPortPlanID)
  }

  func testLoadSSHPlanStoresSelectedVmPlan() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    let plan = SSHPlan(
      vm: "Dev VM",
      user: "ubuntu",
      host: "127.0.0.1",
      port: 2222,
      source: .portForward,
      command: ["ssh", "-p", "2222", "ubuntu@127.0.0.1"]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      sshPlanResult: .success(plan)
    )
    let model = DashboardViewModel(client: client)

    let loaded = await model.loadSSHPlan(user: "  ubuntu  ", for: virtualMachine)

    XCTAssertTrue(loaded)
    XCTAssertEqual(model.sshPlan(for: virtualMachine), plan)
    XCTAssertNil(model.sshPlanError(for: virtualMachine))
    XCTAssertEqual(client.inspectedSSHPlanRequests.count, 1)
    XCTAssertEqual(client.inspectedSSHPlanRequests[0].user, "ubuntu")
    XCTAssertEqual(client.inspectedSSHPlanRequests[0].id, virtualMachine.id)
    XCTAssertNil(model.loadingSSHPlanID)
  }

  func testLoadSSHPlanStoresError() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      sshPlanResult: .failure(TestRefreshError.offline)
    )
    let model = DashboardViewModel(client: client)

    let loaded = await model.loadSSHPlan(user: "ubuntu", for: virtualMachine)

    XCTAssertFalse(loaded)
    XCTAssertNil(model.sshPlan(for: virtualMachine))
    XCTAssertEqual(model.sshPlanError(for: virtualMachine), "Offline")
    XCTAssertEqual(model.alertMessage, "Offline")
    XCTAssertEqual(client.inspectedSSHPlanRequests.map(\.user), ["ubuntu"])
    XCTAssertNil(model.loadingSSHPlanID)
  }

  func testLoadSSHPlanRejectsEmptyUserBeforeClientRequest() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: client)

    let loaded = await model.loadSSHPlan(user: "   ", for: virtualMachine)

    XCTAssertFalse(loaded)
    XCTAssertEqual(model.alertMessage, "Enter an SSH user.")
    XCTAssertTrue(client.inspectedSSHPlanRequests.isEmpty)
    XCTAssertNil(model.loadingSSHPlanID)
  }

  func testLoadNetworkPlanStoresSelectedVmPlan() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    let plan = NetworkPlan(
      vm: "Dev VM",
      backend: "socket-vmnet",
      mode: "shared",
      hostname: "dev-vm.local",
      dryRun: true,
      executable: true,
      portForwards: [VMPortForward(host: 2222, guest: 22)],
      capabilities: NetworkCapabilities(
        guestOutbound: true,
        hostToGuest: true,
        guestToHost: true,
        hostVisibleHostname: true,
        supportsPortForwarding: true,
        requiresPrivilegedHelper: false
      ),
      blockers: [],
      notes: ["Metadata only; no network mutation."]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      networkPlanResult: .success(plan)
    )
    let model = DashboardViewModel(client: client)

    await model.loadNetworkPlan(for: virtualMachine)

    XCTAssertEqual(model.networkPlan(for: virtualMachine), plan)
    XCTAssertNil(model.networkPlanError(for: virtualMachine))
    XCTAssertEqual(client.loadedNetworkPlanIDs, [virtualMachine.id])
    XCTAssertNil(model.loadingNetworkPlanID)
  }

  func testLoadNetworkPlanStoresErrorAndUpdateClientResetsSlice() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      networkPlanResult: .failure(TestRefreshError.offline)
    )
    let model = DashboardViewModel(client: client)

    await model.loadNetworkPlan(for: virtualMachine)

    XCTAssertNil(model.networkPlan(for: virtualMachine))
    XCTAssertEqual(model.networkPlanError(for: virtualMachine), "Offline")
    XCTAssertEqual(model.alertMessage, "Offline")
    XCTAssertEqual(client.loadedNetworkPlanIDs, [virtualMachine.id])
    XCTAssertNil(model.loadingNetworkPlanID)

    model.updateClient(
      StubVirtualMachineClient(
        sourceTitle: "Replacement inventory",
        listResult: .success([])
      )
    )

    XCTAssertNil(model.networkPlan(for: virtualMachine))
    XCTAssertNil(model.networkPlanError(for: virtualMachine))
    XCTAssertNil(model.loadingNetworkPlanID)
  }

  func testLoadPortForwardsStoresManifestList() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    let list = VMPortForwardList(
      vm: "Dev VM",
      forwards: [VMPortForward(host: 2222, guest: 22)]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      portForwardListResult: .success(list)
    )
    let model = DashboardViewModel(client: client)

    await model.loadPortForwards(for: virtualMachine)

    XCTAssertEqual(model.portForwardList(for: virtualMachine), list)
    XCTAssertNil(model.portForwardError(for: virtualMachine))
    XCTAssertEqual(client.listedPortForwardIDs, [virtualMachine.id])
    XCTAssertNil(model.loadingPortForwardsID)
  }

  func testAddPortForwardStoresManifestListAndClearsOpenPlan() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    let plan = OpenPortPlan(
      vm: "Dev VM",
      scheme: "http",
      host: "127.0.0.1",
      guestPort: 3000,
      hostPort: 3000,
      url: "http://127.0.0.1:3000",
      command: ["open", "http://127.0.0.1:3000"]
    )
    let list = VMPortForwardList(
      vm: "Dev VM",
      forwards: [VMPortForward(host: 3000, guest: 3000)]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      openPortPlanResult: .success(plan),
      portForwardListResult: .success(list)
    )
    let model = DashboardViewModel(client: client)

    _ = await model.loadOpenPortPlan(guestPort: "3000", scheme: "http", for: virtualMachine)
    let added = await model.addPortForward(host: " 3000 ", guest: "3000", for: virtualMachine)

    XCTAssertTrue(added)
    XCTAssertNil(model.openPortPlan(for: virtualMachine))
    XCTAssertEqual(model.portForwardList(for: virtualMachine), list)
    XCTAssertNil(model.portForwardError(for: virtualMachine))
    XCTAssertEqual(client.addedPortForwardRequests.count, 1)
    XCTAssertEqual(client.addedPortForwardRequests[0].host, 3000)
    XCTAssertEqual(client.addedPortForwardRequests[0].guest, 3000)
    XCTAssertEqual(client.addedPortForwardRequests[0].id, virtualMachine.id)
    XCTAssertEqual(model.alertMessage, "Port forward 3000:3000 added to the VM manifest.")
    XCTAssertNil(model.addingPortForwardID)
  }

  func testRemovePortForwardStoresManifestListAndClearsOpenPlan() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    let plan = OpenPortPlan(
      vm: "Dev VM",
      scheme: "http",
      host: "127.0.0.1",
      guestPort: 3000,
      hostPort: 3000,
      url: "http://127.0.0.1:3000",
      command: ["open", "http://127.0.0.1:3000"]
    )
    let list = VMPortForwardList(vm: "Dev VM", forwards: [])
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      openPortPlanResult: .success(plan),
      portForwardListResult: .success(list)
    )
    let model = DashboardViewModel(client: client)

    _ = await model.loadOpenPortPlan(guestPort: "3000", scheme: "http", for: virtualMachine)
    let removed = await model.removePortForward(host: 3000, guest: 3000, for: virtualMachine)

    XCTAssertTrue(removed)
    XCTAssertNil(model.openPortPlan(for: virtualMachine))
    XCTAssertEqual(model.portForwardList(for: virtualMachine), list)
    XCTAssertEqual(client.removedPortForwardRequests.count, 1)
    XCTAssertEqual(client.removedPortForwardRequests[0].host, 3000)
    XCTAssertEqual(client.removedPortForwardRequests[0].guest, 3000)
    XCTAssertEqual(model.alertMessage, "Port forward 3000:3000 removed from the VM manifest.")
    XCTAssertNil(model.removingPortForwardID)
  }

  func testAddPortForwardRejectsInvalidPortsBeforeClientRequest() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: client)

    let added = await model.addPortForward(host: "0", guest: "3000", for: virtualMachine)

    XCTAssertFalse(added)
    XCTAssertEqual(model.alertMessage, "Enter valid host and guest ports from 1 to 65535.")
    XCTAssertTrue(client.addedPortForwardRequests.isEmpty)
    XCTAssertNil(model.addingPortForwardID)
  }

  func testLoadBootMediaStatusStoresSelectedVmStatus() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let status = BootMediaStatus(
      vm: "Dev VM",
      entries: [
        BootMediaStatusEntry(
          kind: .installerImage,
          path: "installers/ubuntu-arm64.iso",
          exists: true,
          sizeBytes: 14,
          lastImport: nil,
          lastVerification: nil,
          lastDownloadPlan: nil,
          lastDownload: nil
        )
      ]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      bootMediaStatusResult: .success(status)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.loadBootMediaStatus(for: virtualMachine)

    XCTAssertEqual(model.bootMediaStatus(for: virtualMachine), status)
    XCTAssertNil(model.bootMediaStatusError(for: virtualMachine))
    XCTAssertEqual(client.inspectedBootMediaStatusIDs, [virtualMachine.id])
  }

  func testLoadReadinessReportStoresAggregateAndWarmsReadinessCaches() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let bootMedia = BootMediaStatus(
      vm: "Dev VM",
      entries: [
        BootMediaStatusEntry(
          kind: .installerImage,
          path: "installers/ubuntu-arm64.iso",
          exists: false,
          sizeBytes: nil,
          lastImport: nil,
          lastVerification: nil,
          lastDownloadPlan: nil,
          lastDownload: nil
        )
      ]
    )
    let snapshotChain = VMSnapshotChain(
      activeDisk: VMActiveDisk(
        source: "primary",
        snapshot: nil,
        path: "disks/root.qcow2",
        format: "qcow2",
        exists: false,
        activatedAtUnix: 1_710_000_000
      ),
      disks: []
    )
    let runner = RunnerStatus(
      engine: "lightvm",
      pid: nil,
      command: ["lightvm-runner", "Dev VM", "--apple-vz"],
      logPath: "logs/lightvm.log",
      startedAtUnix: 1_710_000_100,
      dryRun: true,
      launchSpecPath: ".vmbridge/metadata/apple-vz-launch.json",
      launchReadiness: LaunchReadiness(
        ready: false,
        blockers: [
          LaunchReadinessBlocker(
            code: "missing-primary-disk",
            message: "Primary disk is missing.",
            path: "disks/root.qcow2",
            capability: "apple-vz"
          )
        ]
      )
    )
    let report = VMReadinessReport(
      vm: "Dev VM",
      mode: .fast,
      state: .stopped,
      metadataOnly: true,
      liveE2ERequired: true,
      evidenceRequirements: [],
      bootMedia: bootMedia,
      bootMediaError: nil,
      snapshotChain: snapshotChain,
      snapshotChainError: nil,
      runner: runner,
      runnerError: nil,
      blockers: ["boot-media-missing:installers/ubuntu-arm64.iso"],
      notes: ["metadata-only report"]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      readinessReportResult: .success(report)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.loadReadinessReport(for: virtualMachine)

    XCTAssertEqual(model.readinessReport(for: virtualMachine), report)
    XCTAssertNil(model.readinessReportError(for: virtualMachine))
    XCTAssertEqual(model.bootMediaStatus(for: virtualMachine), bootMedia)
    XCTAssertEqual(model.snapshotChain(for: virtualMachine), snapshotChain)
    XCTAssertEqual(model.runnerStatus(for: virtualMachine), runner)
    XCTAssertEqual(client.inspectedReadinessReportIDs, [virtualMachine.id])
  }

  func testLoadReadinessReportPreservesMetadataOnlyAggregateAfterCacheWarming() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let bootMedia = BootMediaStatus(
      vm: "Dev VM",
      entries: [
        BootMediaStatusEntry(
          kind: .installerImage,
          path: "installers/ubuntu-arm64.iso",
          exists: true,
          sizeBytes: 2_048,
          lastImport: nil,
          lastVerification: nil,
          lastDownloadPlan: nil,
          lastDownload: nil
        )
      ]
    )
    let snapshotChain = VMSnapshotChain(
      activeDisk: VMActiveDisk(
        source: "primary",
        snapshot: nil,
        path: "disks/root.qcow2",
        format: "qcow2",
        exists: true,
        activatedAtUnix: 1_710_000_000
      ),
      disks: []
    )
    let report = VMReadinessReport(
      vm: "Dev VM",
      mode: .fast,
      state: .stopped,
      metadataOnly: true,
      liveE2ERequired: false,
      evidenceRequirements: [],
      bootMedia: bootMedia,
      bootMediaError: nil,
      snapshotChain: snapshotChain,
      snapshotChainError: nil,
      runner: nil,
      runnerError: "metadata-only report; runner was not inspected",
      blockers: [],
      notes: [
        "metadata-only report",
        "cache warming reused daemon aggregate details",
      ]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      readinessReportResult: .success(report)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.loadReadinessReport(for: virtualMachine)

    XCTAssertEqual(
      model.readinessReport(for: virtualMachine)?.readinessTitle,
      "Metadata checks clear")
    XCTAssertEqual(
      model.readinessReport(for: virtualMachine)?.notes,
      [
        "metadata-only report",
        "cache warming reused daemon aggregate details",
      ])
    XCTAssertEqual(model.bootMediaStatus(for: virtualMachine), bootMedia)
    XCTAssertEqual(model.snapshotChain(for: virtualMachine), snapshotChain)
    XCTAssertEqual(model.snapshotChain(for: virtualMachine)?.readinessTitle, "Primary disk active")
    XCTAssertEqual(model.runnerStatusError(for: virtualMachine), "metadata-only report; runner was not inspected")
    XCTAssertEqual(client.inspectedReadinessReportIDs, [virtualMachine.id])
  }

  func testCardSummaryUsesLoadedAggregateReadinessPendingLiveEvidenceLabel() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
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
          kind: "guest-tools-effects",
          required: true,
          proven: false,
          note: "Guest-side command effects have not been observed."
        ),
      ],
      bootMedia: nil,
      bootMediaError: nil,
      snapshotChain: nil,
      snapshotChainError: nil,
      runner: nil,
      runnerError: nil,
      blockers: [],
      notes: ["daemon aggregate reused cached metadata"]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      readinessReportResult: .success(report)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.loadReadinessReport(for: virtualMachine)

    let summary = model.cardSummary(for: virtualMachine)

    XCTAssertEqual(
      summary.metadataItems.first,
      "Metadata checks clear; 2 live evidence checks pending: Live boot, Guest tools effects")
    XCTAssertTrue(summary.metadataItems.contains("Guest tools effects unproven"))
    XCTAssertFalse(summary.metadataItems.joined(separator: " ").contains("daemon aggregate"))
    XCTAssertEqual(client.inspectedReadinessReportIDs, [virtualMachine.id])
  }

  func testCardSummaryUsesLoadedAggregateReadinessProvenGuestToolsEffectsLabel()
    async throws
  {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let report = VMReadinessReport(
      vm: "Dev VM",
      mode: .fast,
      state: .stopped,
      metadataOnly: true,
      liveE2ERequired: true,
      evidenceRequirements: [
        VMEvidenceRequirement(
          kind: "guest-tools-effects",
          required: true,
          proven: true,
          note: "Verified guest-tools command/effect evidence from the preserved live bundle."
        )
      ],
      bootMedia: nil,
      bootMediaError: nil,
      snapshotChain: nil,
      snapshotChainError: nil,
      runner: nil,
      runnerError: nil,
      blockers: [],
      notes: []
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      readinessReportResult: .success(report)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.loadReadinessReport(for: virtualMachine)

    let summary = model.cardSummary(for: virtualMachine)

    XCTAssertEqual(
      summary.metadataItems.first,
      "Metadata checks clear; live E2E evidence still required")
    XCTAssertTrue(summary.metadataItems.contains("Guest tools effects proven"))
    XCTAssertFalse(summary.metadataItems.contains("Guest tools effects unproven"))
    XCTAssertEqual(client.inspectedReadinessReportIDs, [virtualMachine.id])
  }

  func testLoadGuestToolsStatusStoresSelectedVmStatus() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let status = GuestToolsStatus(
      vm: "Dev VM",
      tools: "required",
      tokenCreatedAtUnix: 1_710_000_000,
      capabilities: [
        GuestToolsCapability(name: "heartbeat", maxVersion: 1, enabledBy: "base"),
        GuestToolsCapability(name: "display-resize", maxVersion: 1, enabledBy: "display"),
        GuestToolsCapability(name: "clipboard", maxVersion: 1, enabledBy: "integration.clipboard"),
        GuestToolsCapability(
          name: "shared-folders", maxVersion: 1, enabledBy: "integration.shared_folders"),
      ],
      approvedSharedFolders: [
        GuestToolsApprovedSharedFolder(
          name: "workspace",
          hostPath: "/Users/dev/workspace",
          hostPathToken: "host-token-1",
          readOnly: false,
          approval: "required"
        )
      ],
      runtime: GuestToolsRuntime(
        connected: true,
        guestOS: "ubuntu",
        agentVersion: "0.1.0",
        capabilities: ["heartbeat", "display-resize", "clipboard", "shared-folders"],
        lastHeartbeatAtUnix: 1_710_000_060,
        guestIPAddresses: [
          GuestToolsIPAddress(address: "192.168.64.23", interface: "en0")
        ],
        sharedFolders: [
          GuestToolsSharedFolder(name: "workspace", hostPathToken: "host-token-1")
        ],
        metrics: GuestToolsMetrics(
          cpuPercent: 17,
          memoryUsedMiB: 512,
          updatedAtUnix: 1_710_000_061
        ),
        lastCommandResult: GuestToolsCommandResult(
          requestID: "req-clipboard-1",
          capability: "clipboard",
          ok: true,
          errorCode: nil,
          message: "Clipboard updated",
          completedAtUnix: 1_710_000_062
        ),
        updatedAtUnix: 1_710_000_061,
        agentUpdate: GuestToolsAgentUpdate(
          currentVersion: "0.1.0",
          availableVersion: "0.2.0",
          downloadURL: nil,
          signature: nil,
          observedAtUnix: 1_710_000_063
        )
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(status)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.loadGuestToolsStatus(for: virtualMachine)

    XCTAssertEqual(model.guestToolsStatus(for: virtualMachine), status)
    XCTAssertNil(model.guestToolsStatusError(for: virtualMachine))
    XCTAssertEqual(model.guestToolsStatus(for: virtualMachine)?.primaryIPAddress, "192.168.64.23")
    XCTAssertEqual(
      model.guestToolsStatus(for: virtualMachine)?.networkReadinessTitle, "Guest IP ready")
    XCTAssertEqual(
      model.guestToolsStatus(for: virtualMachine)?.displayReadinessTitle,
      "Runtime advertises resize")
    XCTAssertEqual(
      model.guestToolsStatus(for: virtualMachine)?.clipboardReadinessTitle,
      "Runtime advertises clipboard")
    XCTAssertEqual(
      model.guestToolsStatus(for: virtualMachine)?.sharedFoldersReadinessTitle,
      "Runtime advertises shares")
    XCTAssertEqual(
      model.guestToolsStatus(for: virtualMachine)?.approvedSharedFoldersTitle, "Approved (1)")
    XCTAssertEqual(
      model.guestToolsStatus(for: virtualMachine)?.mountReadinessTitle(
        for: status.approvedSharedFolders[0]),
      "Mount command available"
    )
    XCTAssertEqual(
      model.guestToolsStatus(for: virtualMachine)?.approvedSharedFolders,
      [
        GuestToolsApprovedSharedFolder(
          name: "workspace",
          hostPath: "/Users/dev/workspace",
          hostPathToken: "host-token-1",
          readOnly: false,
          approval: "required"
        )
      ]
    )
    XCTAssertEqual(
      model.guestToolsStatus(for: virtualMachine)?.runtime?.sharedFolders,
      [GuestToolsSharedFolder(name: "workspace", hostPathToken: "host-token-1")]
    )
    XCTAssertEqual(
      model.guestToolsStatus(for: virtualMachine)?.runtime?.lastCommandResult,
      GuestToolsCommandResult(
        requestID: "req-clipboard-1",
        capability: "clipboard",
        ok: true,
        errorCode: nil,
        message: "Clipboard updated",
        completedAtUnix: 1_710_000_062
      )
    )
    XCTAssertEqual(
      model.guestToolsStatus(for: virtualMachine)?.runtime?.agentUpdate,
      GuestToolsAgentUpdate(
        currentVersion: "0.1.0",
        availableVersion: "0.2.0",
        downloadURL: nil,
        signature: nil,
        observedAtUnix: 1_710_000_063
      )
    )
    XCTAssertEqual(client.inspectedGuestToolsStatusIDs, [virtualMachine.id])
  }

  func testLoadSharedFoldersStoresSelectedVmShares() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let shares = VMSharedFolderList(
      vm: "Dev VM",
      sharedFolders: [
        VMSharedFolder(
          name: "workspace",
          hostPath: "/Users/dev/workspace",
          readOnly: false,
          hostPathToken: "host-token-1"
        )
      ]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      sharedFolderListResult: .success(shares)
    )
    let model = DashboardViewModel(client: client)

    await model.loadSharedFolders(for: virtualMachine)

    XCTAssertEqual(model.sharedFolderList(for: virtualMachine), shares)
    XCTAssertNil(model.sharedFolderError(for: virtualMachine))
    XCTAssertEqual(client.listedSharedFolderIDs, [virtualMachine.id])
    XCTAssertNil(model.loadingSharedFoldersID)
  }

  func testAddSharedFolderStoresReturnedListRefreshesGuestToolsStatusAndShowsAlert()
    async throws
  {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let shares = VMSharedFolderList(
      vm: "Dev VM",
      sharedFolders: [
        VMSharedFolder(
          name: "workspace",
          hostPath: "/Users/dev/workspace",
          readOnly: true,
          hostPathToken: "host-token-1"
        )
      ]
    )
    let status = DashboardViewModelTests.guestToolsStatus(vm: "Dev VM")
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(status),
      sharedFolderListResult: .success(shares)
    )
    let model = DashboardViewModel(client: client)

    let didAdd = await model.addSharedFolder(
      name: "  workspace  ",
      hostPath: "  /Users/dev/workspace  ",
      readOnly: true,
      hostPathToken: "  host-token-1  ",
      for: virtualMachine
    )

    XCTAssertTrue(didAdd)
    XCTAssertEqual(model.sharedFolderList(for: virtualMachine), shares)
    XCTAssertNil(model.sharedFolderError(for: virtualMachine))
    XCTAssertEqual(model.guestToolsStatus(for: virtualMachine), status)
    XCTAssertEqual(client.addedSharedFolderRequests.count, 1)
    XCTAssertEqual(client.addedSharedFolderRequests[0].name, "workspace")
    XCTAssertEqual(client.addedSharedFolderRequests[0].hostPath, "/Users/dev/workspace")
    XCTAssertTrue(client.addedSharedFolderRequests[0].readOnly)
    XCTAssertEqual(client.addedSharedFolderRequests[0].hostPathToken, "host-token-1")
    XCTAssertEqual(client.addedSharedFolderRequests[0].id, virtualMachine.id)
    XCTAssertEqual(client.inspectedGuestToolsStatusIDs, [virtualMachine.id])
    XCTAssertNil(model.addingSharedFolderID)
    XCTAssertEqual(model.alertMessage, "Shared folder 'workspace' added to the VM manifest.")
  }

  func testRemoveSharedFolderStoresReturnedListRefreshesGuestToolsStatusAndShowsAlert()
    async throws
  {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let shares = VMSharedFolderList(vm: "Dev VM", sharedFolders: [])
    let status = DashboardViewModelTests.guestToolsStatus(vm: "Dev VM")
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(status),
      sharedFolderListResult: .success(shares)
    )
    let model = DashboardViewModel(client: client)

    let didRemove = await model.removeSharedFolder(
      named: "  workspace  ",
      for: virtualMachine
    )

    XCTAssertTrue(didRemove)
    XCTAssertEqual(model.sharedFolderList(for: virtualMachine), shares)
    XCTAssertNil(model.sharedFolderError(for: virtualMachine))
    XCTAssertEqual(model.guestToolsStatus(for: virtualMachine), status)
    XCTAssertEqual(client.removedSharedFolderRequests.count, 1)
    XCTAssertEqual(client.removedSharedFolderRequests[0].shareName, "workspace")
    XCTAssertEqual(client.removedSharedFolderRequests[0].id, virtualMachine.id)
    XCTAssertEqual(client.inspectedGuestToolsStatusIDs, [virtualMachine.id])
    XCTAssertNil(model.removingSharedFolderID)
    XCTAssertEqual(model.alertMessage, "Shared folder 'workspace' removed from the VM manifest.")
  }

  func testSharedFolderManifestActionsRejectInvalidInputsBeforeDispatch() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: client)

    let didAddWithoutName = await model.addSharedFolder(
      name: "   ",
      hostPath: "/Users/dev/workspace",
      readOnly: false,
      hostPathToken: "",
      for: virtualMachine
    )
    XCTAssertFalse(didAddWithoutName)
    XCTAssertEqual(model.alertMessage, "Enter a shared folder name.")

    let didAddWithoutHostPath = await model.addSharedFolder(
      name: "workspace",
      hostPath: "   ",
      readOnly: false,
      hostPathToken: "",
      for: virtualMachine
    )
    XCTAssertFalse(didAddWithoutHostPath)
    XCTAssertEqual(model.alertMessage, "Enter a host path for the shared folder.")

    XCTAssertTrue(client.addedSharedFolderRequests.isEmpty)
    XCTAssertTrue(client.inspectedGuestToolsStatusIDs.isEmpty)
    XCTAssertNil(model.addingSharedFolderID)
  }

  func testMountApprovedSharedFolderStoresReturnedGuestToolsStatus() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let status = GuestToolsStatus(
      vm: "Dev VM",
      tools: "required",
      tokenCreatedAtUnix: 1_710_000_000,
      capabilities: [
        GuestToolsCapability(
          name: "shared-folders", maxVersion: 1, enabledBy: "integration.shared_folders")
      ],
      approvedSharedFolders: [
        GuestToolsApprovedSharedFolder(
          name: "workspace",
          hostPath: "/Users/dev/workspace",
          hostPathToken: "host-token-1",
          readOnly: false,
          approval: "required"
        )
      ],
      runtime: GuestToolsRuntime(
        connected: true,
        guestOS: "ubuntu",
        agentVersion: "0.1.0",
        capabilities: ["shared-folders"],
        lastHeartbeatAtUnix: 1_710_000_060,
        guestIPAddresses: [],
        sharedFolders: [
          GuestToolsSharedFolder(
            name: "workspace",
            hostPathToken: "host-token-1",
            mountedAtUnix: 1_710_000_062
          )
        ],
        metrics: nil,
        updatedAtUnix: 1_710_000_062
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(status)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didMount = await model.mountApprovedSharedFolder(
      named: "  workspace  ",
      for: virtualMachine
    )

    XCTAssertTrue(didMount)
    XCTAssertEqual(client.mountedApprovedSharedFolderRequests.count, 1)
    XCTAssertEqual(client.mountedApprovedSharedFolderRequests[0].shareName, "workspace")
    XCTAssertEqual(client.mountedApprovedSharedFolderRequests[0].id, virtualMachine.id)
    XCTAssertEqual(model.guestToolsStatus(for: virtualMachine), status)
    XCTAssertNil(model.guestToolsStatusError(for: virtualMachine))
    XCTAssertEqual(
      model.guestToolsStatus(for: virtualMachine)?.mountReadinessTitle(
        for: status.approvedSharedFolders[0]),
      "Mounted"
    )
  }

  func testUnmountApprovedSharedFolderStoresReturnedGuestToolsStatus() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let status = GuestToolsStatus(
      vm: "Dev VM",
      tools: "required",
      tokenCreatedAtUnix: 1_710_000_000,
      capabilities: [
        GuestToolsCapability(
          name: "shared-folders", maxVersion: 1, enabledBy: "integration.shared_folders")
      ],
      approvedSharedFolders: [
        GuestToolsApprovedSharedFolder(
          name: "workspace",
          hostPath: "/Users/dev/workspace",
          hostPathToken: "host-token-1",
          readOnly: false,
          approval: "required"
        )
      ],
      runtime: GuestToolsRuntime(
        connected: true,
        guestOS: "ubuntu",
        agentVersion: "0.1.0",
        capabilities: ["shared-folders"],
        lastHeartbeatAtUnix: 1_710_000_060,
        guestIPAddresses: [],
        sharedFolders: [],
        metrics: nil,
        updatedAtUnix: 1_710_000_064
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(status)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didUnmount = await model.unmountApprovedSharedFolder(
      named: "  workspace  ",
      for: virtualMachine
    )

    XCTAssertTrue(didUnmount)
    XCTAssertEqual(client.unmountedApprovedSharedFolderRequests.count, 1)
    XCTAssertEqual(client.unmountedApprovedSharedFolderRequests[0].shareName, "workspace")
    XCTAssertEqual(client.unmountedApprovedSharedFolderRequests[0].id, virtualMachine.id)
    XCTAssertEqual(model.guestToolsStatus(for: virtualMachine), status)
    XCTAssertNil(model.guestToolsStatusError(for: virtualMachine))
    XCTAssertEqual(
      model.guestToolsStatus(for: virtualMachine)?.mountReadinessTitle(
        for: status.approvedSharedFolders[0]),
      "Mount command available"
    )
  }

  func testApprovedSharedFolderActionsBlockWhenRuntimeCapabilityIsMissing() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let status = GuestToolsStatus(
      vm: "Dev VM",
      tools: "required",
      tokenCreatedAtUnix: 1_710_000_000,
      capabilities: [
        GuestToolsCapability(
          name: "shared-folders", maxVersion: 1, enabledBy: "integration.shared_folders")
      ],
      approvedSharedFolders: [
        GuestToolsApprovedSharedFolder(
          name: "workspace",
          hostPath: "/Users/dev/workspace",
          hostPathToken: "host-token-1",
          readOnly: false,
          approval: "required"
        )
      ],
      runtime: GuestToolsRuntime(
        connected: true,
        guestOS: "ubuntu",
        agentVersion: "0.1.0",
        capabilities: ["clipboard"],
        lastHeartbeatAtUnix: 1_710_000_060,
        guestIPAddresses: [],
        sharedFolders: [],
        metrics: nil,
        updatedAtUnix: 1_710_000_064
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(status)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didMount = await model.mountApprovedSharedFolder(
      named: "workspace",
      for: virtualMachine
    )
    let didUnmount = await model.unmountApprovedSharedFolder(
      named: "workspace",
      for: virtualMachine
    )

    XCTAssertFalse(didMount)
    XCTAssertFalse(didUnmount)
    XCTAssertEqual(client.inspectedGuestToolsStatusIDs, [virtualMachine.id])
    XCTAssertTrue(client.mountedApprovedSharedFolderRequests.isEmpty)
    XCTAssertTrue(client.unmountedApprovedSharedFolderRequests.isEmpty)
    XCTAssertEqual(
      model.alertMessage,
      "Guest tools command blocked for Dev VM: runtime capability shared-folders is not advertised."
    )
  }

  func testSendGuestToolsCommandQueuesCommandAndRefreshesStatus() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let commandResult = GuestToolsCommandResult(
      requestID: "apps-1710000062",
      capability: "applications",
      ok: true,
      errorCode: nil,
      message: "Applications listed",
      completedAtUnix: 1_710_000_062
    )
    let status = GuestToolsStatus(
      vm: "Dev VM",
      tools: "required",
      tokenCreatedAtUnix: 1_710_000_000,
      capabilities: [
        GuestToolsCapability(
          name: "applications", maxVersion: 1, enabledBy: "integration.applications")
      ],
      runtime: GuestToolsRuntime(
        connected: true,
        guestOS: "ubuntu",
        agentVersion: "0.1.0",
        capabilities: ["applications"],
        lastHeartbeatAtUnix: 1_710_000_060,
        guestIPAddresses: [],
        sharedFolders: [],
        metrics: nil,
        lastCommandResult: commandResult,
        updatedAtUnix: 1_710_000_062
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(status),
      guestToolsCommandDispatchResult: .success(
        GuestToolsCommandDispatch(vm: "Dev VM", requestID: "apps-fixed", pendingCommands: 1)
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didSend = await model.sendGuestToolsCommand(
      .listApplications,
      requestID: "apps-fixed",
      for: virtualMachine
    )

    XCTAssertTrue(didSend)
    XCTAssertNil(model.sendingGuestToolsCommandID)
    XCTAssertEqual(client.sentGuestToolsCommandRequests.count, 1)
    XCTAssertEqual(client.sentGuestToolsCommandRequests[0].command, .listApplications)
    XCTAssertEqual(client.sentGuestToolsCommandRequests[0].requestID, "apps-fixed")
    XCTAssertEqual(client.sentGuestToolsCommandRequests[0].id, virtualMachine.id)
    XCTAssertEqual(client.inspectedGuestToolsStatusIDs, [virtualMachine.id, virtualMachine.id])
    XCTAssertEqual(
      model.guestToolsCommandDispatch(for: virtualMachine),
      GuestToolsCommandDispatch(vm: "Dev VM", requestID: "apps-fixed", pendingCommands: 1)
    )
    XCTAssertEqual(model.guestToolsStatus(for: virtualMachine), status)
    XCTAssertEqual(
      model.guestToolsStatus(for: virtualMachine)?.runtime?.lastCommandResult, commandResult)
    XCTAssertNil(model.guestToolsStatusError(for: virtualMachine))
  }

  func testSendGuestToolsCommandBlocksWhenRuntimeCapabilityIsMissing() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let status = GuestToolsStatus(
      vm: "Dev VM",
      tools: "required",
      tokenCreatedAtUnix: 1_710_000_000,
      capabilities: [
        GuestToolsCapability(
          name: "applications", maxVersion: 1, enabledBy: "integration.applications")
      ],
      runtime: GuestToolsRuntime(
        connected: true,
        guestOS: "ubuntu",
        agentVersion: "0.1.0",
        capabilities: ["windows"],
        lastHeartbeatAtUnix: 1_710_000_060,
        guestIPAddresses: [],
        sharedFolders: [],
        metrics: nil,
        updatedAtUnix: 1_710_000_062
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(status),
      guestToolsCommandDispatchResult: .success(
        GuestToolsCommandDispatch(vm: "Dev VM", requestID: "apps-fixed", pendingCommands: 1)
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didSend = await model.sendGuestToolsCommand(
      .listApplications,
      requestID: "apps-fixed",
      for: virtualMachine
    )

    XCTAssertFalse(didSend)
    XCTAssertEqual(client.inspectedGuestToolsStatusIDs, [virtualMachine.id])
    XCTAssertTrue(client.sentGuestToolsCommandRequests.isEmpty)
    XCTAssertEqual(
      model.alertMessage,
      "Guest tools command blocked for Dev VM: runtime capability applications is not advertised."
    )
  }

  func testSendGuestToolsCommandGeneratesTraceableRequestID() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let status = GuestToolsStatus(
      vm: "Dev VM",
      tools: "required",
      tokenCreatedAtUnix: 1_710_000_000,
      capabilities: [],
      runtime: GuestToolsRuntime(
        connected: true,
        guestOS: "ubuntu",
        agentVersion: "0.1.0",
        capabilities: ["windows"],
        lastHeartbeatAtUnix: 1_710_000_060,
        guestIPAddresses: [],
        sharedFolders: [],
        metrics: nil,
        updatedAtUnix: 1_710_000_062
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(status),
      guestToolsCommandDispatchResult: .success(
        GuestToolsCommandDispatch(vm: "Dev VM", requestID: "windows-generated", pendingCommands: 1)
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didSend = await model.sendGuestToolsCommand(
      GuestToolsAgentCommand.listWindows, for: virtualMachine)

    XCTAssertTrue(didSend)
    XCTAssertEqual(client.sentGuestToolsCommandRequests.count, 1)
    XCTAssertEqual(
      client.sentGuestToolsCommandRequests[0].command, GuestToolsAgentCommand.listWindows)
    XCTAssertTrue(client.sentGuestToolsCommandRequests[0].requestID?.hasPrefix("windows-") == true)
    XCTAssertEqual(client.sentGuestToolsCommandRequests[0].id, virtualMachine.id)
  }

  func testSyncGuestTimeSendsCurrentTimeSyncCommandAndRefreshesStatus() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let status = GuestToolsStatus(
      vm: "Dev VM",
      tools: "required",
      tokenCreatedAtUnix: 1_710_000_000,
      capabilities: [
        GuestToolsCapability(name: "time-sync", maxVersion: 1, enabledBy: "base")
      ],
      runtime: GuestToolsRuntime(
        connected: true,
        guestOS: "ubuntu",
        agentVersion: "0.1.0",
        capabilities: ["time-sync"],
        lastHeartbeatAtUnix: 1_710_000_060,
        guestIPAddresses: [],
        sharedFolders: [],
        metrics: nil,
        updatedAtUnix: 1_710_000_062
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(status),
      guestToolsCommandDispatchResult: .success(
        GuestToolsCommandDispatch(
          vm: "Dev VM", requestID: "time-sync-generated", pendingCommands: 1)
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didSend = await model.syncGuestTime(for: virtualMachine)

    XCTAssertTrue(didSend)
    XCTAssertEqual(client.sentGuestToolsCommandRequests.count, 1)
    guard case .timeSync(let unixEpochMillis) = client.sentGuestToolsCommandRequests[0].command
    else {
      return XCTFail("Expected time sync command")
    }
    XCTAssertGreaterThan(unixEpochMillis, 0)
    XCTAssertTrue(
      client.sentGuestToolsCommandRequests[0].requestID?.hasPrefix("time-sync-") == true)
    XCTAssertEqual(client.sentGuestToolsCommandRequests[0].id, virtualMachine.id)
    XCTAssertEqual(client.inspectedGuestToolsStatusIDs, [virtualMachine.id, virtualMachine.id])
  }

  func testSendClipboardTextTrimsInputAndUsesClipboardRequestID() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let status = GuestToolsStatus(
      vm: "Dev VM",
      tools: "required",
      tokenCreatedAtUnix: 1_710_000_000,
      capabilities: [
        GuestToolsCapability(name: "clipboard", maxVersion: 1, enabledBy: "integration.clipboard")
      ],
      runtime: GuestToolsRuntime(
        connected: true,
        guestOS: "ubuntu",
        agentVersion: "0.1.0",
        capabilities: ["clipboard"],
        lastHeartbeatAtUnix: 1_710_000_060,
        guestIPAddresses: [],
        sharedFolders: [],
        metrics: nil,
        updatedAtUnix: 1_710_000_062
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(status),
      guestToolsCommandDispatchResult: .success(
        GuestToolsCommandDispatch(
          vm: "Dev VM", requestID: "clipboard-generated", pendingCommands: 1)
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didSend = await model.sendClipboardText("  hello from mac  ", for: virtualMachine)

    XCTAssertTrue(didSend)
    XCTAssertEqual(client.sentGuestToolsCommandRequests.count, 1)
    XCTAssertEqual(
      client.sentGuestToolsCommandRequests[0].command,
      GuestToolsAgentCommand.setClipboard(text: "hello from mac")
    )
    XCTAssertTrue(
      client.sentGuestToolsCommandRequests[0].requestID?.hasPrefix("clipboard-") == true)
    XCTAssertEqual(client.sentGuestToolsCommandRequests[0].id, virtualMachine.id)
    XCTAssertEqual(client.inspectedGuestToolsStatusIDs, [virtualMachine.id, virtualMachine.id])
  }

  func testResizeDisplayParsesInputsAndUsesDisplayRequestID() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let status = GuestToolsStatus(
      vm: "Dev VM",
      tools: "required",
      tokenCreatedAtUnix: 1_710_000_000,
      capabilities: [
        GuestToolsCapability(name: "display-resize", maxVersion: 1, enabledBy: "display")
      ],
      runtime: GuestToolsRuntime(
        connected: true,
        guestOS: "ubuntu",
        agentVersion: "0.1.0",
        capabilities: ["display-resize"],
        lastHeartbeatAtUnix: 1_710_000_060,
        guestIPAddresses: [],
        sharedFolders: [],
        metrics: nil,
        updatedAtUnix: 1_710_000_062
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(status),
      guestToolsCommandDispatchResult: .success(
        GuestToolsCommandDispatch(vm: "Dev VM", requestID: "display-generated", pendingCommands: 1)
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didSend = await model.resizeDisplay(
      width: " 1440 ",
      height: "900",
      scale: "2",
      for: virtualMachine
    )

    XCTAssertTrue(didSend)
    XCTAssertEqual(client.sentGuestToolsCommandRequests.count, 1)
    XCTAssertEqual(
      client.sentGuestToolsCommandRequests[0].command,
      GuestToolsAgentCommand.resizeDisplay(width: 1440, height: 900, scale: 2)
    )
    XCTAssertTrue(client.sentGuestToolsCommandRequests[0].requestID?.hasPrefix("display-") == true)
    XCTAssertEqual(client.sentGuestToolsCommandRequests[0].id, virtualMachine.id)
    XCTAssertEqual(client.inspectedGuestToolsStatusIDs, [virtualMachine.id, virtualMachine.id])
  }

  func testSendInlineFileDropRejectsInvalidInputsBeforeDispatch() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: client)

    await model.load()

    let didRejectFileName = await model.sendInlineFileDrop(
      fileName: "   ",
      contents: "hello from mac",
      for: virtualMachine
    )
    XCTAssertFalse(didRejectFileName)
    XCTAssertEqual(model.alertMessage, "Enter a file name to drop.")

    let didRejectContents = await model.sendInlineFileDrop(
      fileName: "notes.txt",
      contents: " \n\t ",
      for: virtualMachine
    )
    XCTAssertFalse(didRejectContents)
    XCTAssertEqual(model.alertMessage, "Enter file contents to drop.")

    XCTAssertTrue(client.sentGuestToolsCommandRequests.isEmpty)
    XCTAssertTrue(client.inspectedGuestToolsStatusIDs.isEmpty)
  }

  func testSendInlineFileDropDispatchesStartChunkAndCompleteCommands() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let status = GuestToolsStatus(
      vm: "Dev VM",
      tools: "required",
      tokenCreatedAtUnix: 1_710_000_000,
      capabilities: [
        GuestToolsCapability(name: "drag-drop", maxVersion: 1, enabledBy: "drag-drop")
      ],
      runtime: GuestToolsRuntime(
        connected: true,
        guestOS: "ubuntu",
        agentVersion: "0.1.0",
        capabilities: ["drag-drop"],
        lastHeartbeatAtUnix: 1_710_000_060,
        guestIPAddresses: [],
        sharedFolders: [],
        metrics: nil,
        updatedAtUnix: 1_710_000_062
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(status),
      guestToolsCommandDispatchResult: .success(
        GuestToolsCommandDispatch(
          vm: "Dev VM", requestID: "file-drop-generated", pendingCommands: 1)
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didDrop = await model.sendInlineFileDrop(
      fileName: "  notes.txt  ",
      contents: "  hello\nworld  ",
      for: virtualMachine
    )

    XCTAssertTrue(didDrop)
    XCTAssertNil(model.sendingGuestToolsCommandID)
    XCTAssertEqual(client.sentGuestToolsCommandRequests.count, 3)
    XCTAssertEqual(
      client.sentGuestToolsCommandRequests.map { $0.id },
      [
        virtualMachine.id,
        virtualMachine.id,
        virtualMachine.id,
      ])

    let trimmedContents = "hello\nworld"
    let expectedSizeBytes = UInt64(Data(trimmedContents.utf8).count)

    guard
      case .fileDropStart(let startTransferID, let fileName, let sizeBytes) =
        client.sentGuestToolsCommandRequests[0].command
    else {
      return XCTFail("Expected file drop start command first.")
    }
    XCTAssertFalse(startTransferID.isEmpty)
    XCTAssertEqual(fileName, "notes.txt")
    XCTAssertEqual(sizeBytes, expectedSizeBytes)

    guard
      case .fileDropChunk(let chunkTransferID, let chunkIndex, let dataBase64) =
        client.sentGuestToolsCommandRequests[1].command
    else {
      return XCTFail("Expected file drop chunk command second.")
    }
    XCTAssertEqual(chunkTransferID, startTransferID)
    XCTAssertEqual(chunkIndex, 0)
    let decodedContents = Data(base64Encoded: dataBase64)
      .flatMap { String(data: $0, encoding: .utf8) }
    XCTAssertEqual(decodedContents, trimmedContents)

    guard
      case .fileDropComplete(let completeTransferID) =
        client.sentGuestToolsCommandRequests[2].command
    else {
      return XCTFail("Expected file drop complete command third.")
    }
    XCTAssertEqual(completeTransferID, startTransferID)
    XCTAssertEqual(
      client.inspectedGuestToolsStatusIDs,
      [
        virtualMachine.id,
        virtualMachine.id,
        virtualMachine.id,
        virtualMachine.id,
      ])
    XCTAssertEqual(model.guestToolsStatus(for: virtualMachine), status)
    XCTAssertNil(model.guestToolsStatusError(for: virtualMachine))
  }

  func testGuestToolsWindowAndApplicationConvenienceActionsTrimIDsAndRefreshStatus()
    async throws
  {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let status = GuestToolsStatus(
      vm: "Dev VM",
      tools: "required",
      tokenCreatedAtUnix: 1_710_000_000,
      capabilities: [
        GuestToolsCapability(name: "applications", maxVersion: 1, enabledBy: "applications"),
        GuestToolsCapability(name: "windows", maxVersion: 1, enabledBy: "windows"),
      ],
      runtime: GuestToolsRuntime(
        connected: true,
        guestOS: "ubuntu",
        agentVersion: "0.1.0",
        capabilities: ["applications", "windows"],
        lastHeartbeatAtUnix: 1_710_000_060,
        guestIPAddresses: [],
        sharedFolders: [],
        metrics: nil,
        updatedAtUnix: 1_710_000_062
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(status),
      guestToolsCommandDispatchResult: .success(
        GuestToolsCommandDispatch(vm: "Dev VM", requestID: "command-generated", pendingCommands: 1)
      )
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didLaunch = await model.launchApplication(
      id: "  org.example.Terminal  ", for: virtualMachine)
    let didFocus = await model.focusWindow(id: "  window-42  ", for: virtualMachine)
    let didClose = await model.closeWindow(id: "  window-99  ", for: virtualMachine)

    XCTAssertTrue(didLaunch)
    XCTAssertTrue(didFocus)
    XCTAssertTrue(didClose)
    XCTAssertEqual(client.sentGuestToolsCommandRequests.count, 3)
    XCTAssertEqual(
      client.sentGuestToolsCommandRequests[0].command,
      GuestToolsAgentCommand.launchApplication(id: "org.example.Terminal")
    )
    XCTAssertTrue(
      client.sentGuestToolsCommandRequests[0].requestID?.hasPrefix("launch-app-") == true)
    XCTAssertEqual(client.sentGuestToolsCommandRequests[0].id, virtualMachine.id)
    XCTAssertEqual(
      client.sentGuestToolsCommandRequests[1].command,
      GuestToolsAgentCommand.focusWindow(id: "window-42")
    )
    XCTAssertTrue(
      client.sentGuestToolsCommandRequests[1].requestID?.hasPrefix("focus-window-") == true)
    XCTAssertEqual(client.sentGuestToolsCommandRequests[1].id, virtualMachine.id)
    XCTAssertEqual(
      client.sentGuestToolsCommandRequests[2].command,
      GuestToolsAgentCommand.closeWindow(id: "window-99")
    )
    XCTAssertTrue(
      client.sentGuestToolsCommandRequests[2].requestID?.hasPrefix("close-window-") == true)
    XCTAssertEqual(client.sentGuestToolsCommandRequests[2].id, virtualMachine.id)
    XCTAssertEqual(
      client.inspectedGuestToolsStatusIDs,
      [
        virtualMachine.id,
        virtualMachine.id,
        virtualMachine.id,
        virtualMachine.id,
      ])
    XCTAssertEqual(model.guestToolsStatus(for: virtualMachine), status)
    XCTAssertNil(model.guestToolsStatusError(for: virtualMachine))
  }

  func testGuestToolsConvenienceActionsRejectInvalidInputsBeforeDispatch() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: client)

    await model.load()

    let didSendClipboard = await model.sendClipboardText("   ", for: virtualMachine)
    XCTAssertFalse(didSendClipboard)
    XCTAssertEqual(model.alertMessage, "Enter clipboard text to send.")

    let didSendOversizedClipboard = await model.sendClipboardText(
      String(repeating: "x", count: 1024 * 1024 + 1),
      for: virtualMachine
    )
    XCTAssertFalse(didSendOversizedClipboard)
    XCTAssertEqual(
      model.alertMessage,
      "Clipboard text is too large. Use no more than 1 MiB of UTF-8 text."
    )

    let didResize = await model.resizeDisplay(
      width: "0", height: "900", scale: "2", for: virtualMachine)
    XCTAssertFalse(didResize)
    XCTAssertEqual(model.alertMessage, "Enter a valid display width.")

    let didResizeOversized = await model.resizeDisplay(
      width: "4096", height: "4096", scale: "2", for: virtualMachine)
    XCTAssertFalse(didResizeOversized)
    XCTAssertEqual(
      model.alertMessage,
      "Display size is too large. Use no more than 32 megapixels including scale."
    )

    let didLaunch = await model.launchApplication(id: "   ", for: virtualMachine)
    XCTAssertFalse(didLaunch)
    XCTAssertEqual(model.alertMessage, "Enter an application ID to launch.")

    let didFocus = await model.focusWindow(id: "   ", for: virtualMachine)
    XCTAssertFalse(didFocus)
    XCTAssertEqual(model.alertMessage, "Enter a window ID to focus.")

    let didClose = await model.closeWindow(id: "   ", for: virtualMachine)
    XCTAssertFalse(didClose)
    XCTAssertEqual(model.alertMessage, "Enter a window ID to close.")

    XCTAssertTrue(client.sentGuestToolsCommandRequests.isEmpty)
    XCTAssertTrue(client.inspectedGuestToolsStatusIDs.isEmpty)
  }

  func testShowDisplayRejectsInvalidDimensionsBeforeLaunchingHelper() {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let model = DashboardViewModel(
      client: StubVirtualMachineClient(
        sourceTitle: "Mock inventory",
        listResult: .success([virtualMachine])
      )
    )

    model.showDisplay(width: "0", height: "900", for: virtualMachine)
    XCTAssertEqual(model.alertMessage, "Enter a valid display window width.")

    model.showDisplay(width: "1440", height: "nope", for: virtualMachine)
    XCTAssertEqual(model.alertMessage, "Enter a valid display window height.")

    model.showDisplay(width: "8192", height: "8192", for: virtualMachine)
    XCTAssertEqual(model.alertMessage, "Display size is too large. Use no more than 32 megapixels.")
  }

  func testShowDisplayRefreshesForegroundRuntimeCachesAfterLaunch() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let runnerStatus = RunnerStatus(
      engine: "apple-vz",
      pid: 4242,
      command: [
        "lightvm-runner",
        "Dev VM",
        "--apple-vz-display",
        "--apple-vz-display-width",
        "1440",
        "--apple-vz-display-height",
        "900",
      ],
      logPath: "logs/lightvm.log",
      startedAtUnix: 1_710_000_100,
      dryRun: false,
      launchSpecPath: ".vmbridge/metadata/apple-vz-launch.json",
      launchReadiness: LaunchReadiness(ready: true, blockers: [])
    )
    let policy = RuntimeResourcePolicy(
      vm: "Dev VM",
      mode: "fast",
      profile: "automatic",
      visibility: .foreground,
      state: "running",
      onBattery: false,
      memory: "4096",
      cpu: "2",
      displayFPSCap: "adaptive",
      rationale: "Foreground display active.",
      liveApplied: false,
      runtimeControlAcknowledged: true,
      liveApplyBlockers: [
        RuntimeResourcePolicyBlocker(
          code: "runtime-control-unavailable",
          message: "Live Apple VZ CPU/RAM hot-apply is not implemented yet; the policy is available to display pacing and runtime policy IPC consumers."
        )
      ],
      updatedAtUnix: 1_710_000_500
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      runnerStatusResult: .success(runnerStatus),
      runtimeResourcePolicyResult: .success(policy)
    )
    var launchedVMName: String?
    var launchedDisplaySize: EmbeddedDisplayLauncher.DisplaySize?
    var launchedStoreMetadata: EmbeddedDisplayLauncher.StoreMetadata?
    let model = DashboardViewModel(
      client: client,
      launchEmbeddedDisplay: { vmName, displaySize, storeMetadata in
        launchedVMName = vmName
        launchedDisplaySize = displaySize
        launchedStoreMetadata = storeMetadata
      }
    )

    await model.load()
    model.showDisplay(width: "1440", height: "900", for: virtualMachine)
    for _ in 0..<20 where model.runtimeResourcePolicy(for: virtualMachine) == nil {
      await Task.yield()
    }

    XCTAssertEqual(launchedVMName, "Dev VM")
    XCTAssertEqual(launchedDisplaySize, EmbeddedDisplayLauncher.DisplaySize(width: 1440, height: 900))
    XCTAssertNil(launchedStoreMetadata)
    XCTAssertEqual(client.inspectedRunnerStatusIDs, [virtualMachine.id])
    XCTAssertEqual(client.reappliedRuntimeResourceRequests.count, 1)
    XCTAssertEqual(client.reappliedRuntimeResourceRequests[0].visibility, .foreground)
    XCTAssertEqual(client.reappliedRuntimeResourceRequests[0].id, virtualMachine.id)
    XCTAssertEqual(model.runnerStatus(for: virtualMachine), runnerStatus)
    XCTAssertEqual(model.runtimeResourcePolicy(for: virtualMachine), policy)
    XCTAssertNil(model.runnerStatusError(for: virtualMachine))
    XCTAssertNil(model.runtimeResourcePolicyError(for: virtualMachine))
    XCTAssertEqual(
      model.alertMessage,
      "Opening an embedded display window for Dev VM at 1440x900 (close the window to stop the VM)."
    )
  }

  func testShowDisplayPassesCustomStoreMetadataWhenClientProvidesIt() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let metadata = EmbeddedDisplayLauncher.StoreMetadata(
      storeRoot: "/Volumes/BridgeVM Store",
      bundlePath: "/Volumes/BridgeVM Store/vms/dev-from-daemon.vmbridge"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "bridgevmd",
      listResult: .success([virtualMachine]),
      displayStoreMetadataByID: [virtualMachine.id: metadata]
    )
    var launchedStoreMetadata: EmbeddedDisplayLauncher.StoreMetadata?
    let model = DashboardViewModel(
      client: client,
      launchEmbeddedDisplay: { _, _, storeMetadata in
        launchedStoreMetadata = storeMetadata
      }
    )

    await model.load()
    model.showDisplay(width: "1280", height: "800", for: virtualMachine)

    XCTAssertEqual(launchedStoreMetadata, metadata)
  }

  func testSendGuestToolsCommandRecordsErrorWithoutRefreshingStatus() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(
        GuestToolsStatus(
          vm: "Dev VM",
          tools: "required",
          tokenCreatedAtUnix: 1_710_000_000,
          capabilities: [
            GuestToolsCapability(
              name: "applications", maxVersion: 1, enabledBy: "integration.applications")
          ],
          runtime: GuestToolsRuntime(
            connected: true,
            guestOS: "ubuntu",
            agentVersion: "0.1.0",
            capabilities: ["applications"],
            lastHeartbeatAtUnix: 1_710_000_060,
            guestIPAddresses: [],
            sharedFolders: [],
            metrics: nil,
            updatedAtUnix: 1_710_000_062
          )
        )
      ),
      guestToolsCommandDispatchResult: .failure(TestRefreshError.offline)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didSend = await model.sendGuestToolsCommand(
      .listApplications,
      requestID: "apps-fixed",
      for: virtualMachine
    )

    XCTAssertFalse(didSend)
    XCTAssertNil(model.sendingGuestToolsCommandID)
    XCTAssertEqual(client.sentGuestToolsCommandRequests.count, 1)
    XCTAssertEqual(client.inspectedGuestToolsStatusIDs, [virtualMachine.id])
    XCTAssertEqual(model.guestToolsStatusError(for: virtualMachine), "Offline")
    XCTAssertEqual(model.alertMessage, "Offline")
  }

  func testLoadRunnerStatusStoresSelectedVmStatusAndLaunchReadiness() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let status = RunnerStatus(
      engine: "lightvm",
      pid: nil,
      command: ["lightvm-runner", "Dev VM", "--apple-vz"],
      logPath: "logs/lightvm.log",
      startedAtUnix: 1_710_000_100,
      dryRun: true,
      launchSpecPath: ".vmbridge/metadata/apple-vz-launch.json",
      launchReadiness: LaunchReadiness(
        ready: false,
        blockers: [
          LaunchReadinessBlocker(
            code: "missing-primary-disk",
            message: "Primary disk is missing.",
            path: "disks/root.qcow2",
            capability: nil
          )
        ]
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      runnerStatusResult: .success(status)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.loadRunnerStatus(for: virtualMachine)

    XCTAssertEqual(model.runnerStatus(for: virtualMachine), status)
    XCTAssertNil(model.runnerStatusError(for: virtualMachine))
    XCTAssertEqual(
      model.runnerStatus(for: virtualMachine)?.commandLine, "lightvm-runner Dev VM --apple-vz")
    XCTAssertEqual(model.runnerStatus(for: virtualMachine)?.launchReadinessTitle, "Blocked (1)")
    XCTAssertEqual(model.runnerStatus(for: virtualMachine)?.launchReadiness?.ready, false)
    XCTAssertEqual(
      model.runnerStatus(for: virtualMachine)?.launchReadiness?.blockers.first?.code,
      "missing-primary-disk")
    XCTAssertEqual(client.inspectedRunnerStatusIDs, [virtualMachine.id])
  }

  func testPrepareRunStoresSelectedVmStatusAndLaunchReadiness() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let status = RunnerStatus(
      engine: "apple-vz",
      pid: nil,
      command: ["bridgevm", "run", "Dev VM", "--backend", "apple-vz"],
      logPath: "logs/apple-vz.log",
      startedAtUnix: 1_710_000_200,
      dryRun: true,
      launchSpecPath: ".vmbridge/metadata/apple-vz-launch.json",
      launchReadiness: LaunchReadiness(
        ready: false,
        blockers: [
          LaunchReadinessBlocker(
            code: "missing-capability",
            message: "Apple VZ capability is not available.",
            path: nil,
            capability: "apple-vz"
          )
        ]
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      prepareRunResult: .success(status),
      runnerStatusResult: .success(nil)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didPrepare = await model.prepareRun(for: virtualMachine)

    XCTAssertTrue(didPrepare)
    XCTAssertEqual(model.runnerStatus(for: virtualMachine), status)
    XCTAssertNil(model.runnerStatusError(for: virtualMachine))
    XCTAssertEqual(model.runnerStatus(for: virtualMachine)?.launchSpecPath, status.launchSpecPath)
    XCTAssertEqual(
      model.runnerStatus(for: virtualMachine)?.launchReadiness?.blockers.first?.capability,
      "apple-vz")
    XCTAssertEqual(client.preparedRunIDs, [virtualMachine.id])
    XCTAssertEqual(model.alertMessage, "Dev VM launch readiness prepared.")
  }

  func testRuntimeControlStatusStoresDisplayControlResponse() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let result = RuntimeControlCommandResult(
      vm: "Dev VM",
      kind: "apple-vz-display",
      socketPath: "/tmp/bvm-vz-test.sock",
      command: "status",
      response: GuestToolsCommandPayload(
        value: .object([
          "ok": .bool(true),
          "state": .string("running"),
        ])
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      runtimeControlResult: .success(result)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didSend = await model.runtimeControlStatus(for: virtualMachine)

    XCTAssertTrue(didSend)
    XCTAssertEqual(model.runtimeControlResult(for: virtualMachine), result)
    XCTAssertNil(model.runtimeControlError(for: virtualMachine))
    XCTAssertEqual(client.sentRuntimeControlCommandRequests.count, 1)
    XCTAssertEqual(client.sentRuntimeControlCommandRequests[0].command, "status")
    XCTAssertEqual(client.sentRuntimeControlCommandRequests[0].id, virtualMachine.id)
  }

  func testRuntimeControlPolicyStoresDisplayPolicyResponse() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let result = RuntimeControlCommandResult(
      vm: "Dev VM",
      kind: "apple-vz-display",
      socketPath: "/tmp/bvm-vz-test.sock",
      command: "policy",
      response: GuestToolsCommandPayload(
        value: .object([
          "ok": .bool(true),
          "policy": .object([
            "visibility": .string("foreground"),
            "display_fps_cap": .string("adaptive"),
          ]),
        ])
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      runtimeControlResult: .success(result)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didSend = await model.runtimeControlPolicy(for: virtualMachine)

    XCTAssertTrue(didSend)
    XCTAssertEqual(model.runtimeControlResult(for: virtualMachine), result)
    XCTAssertNil(model.runtimeControlError(for: virtualMachine))
    XCTAssertEqual(client.sentRuntimeControlCommandRequests.count, 1)
    XCTAssertEqual(client.sentRuntimeControlCommandRequests[0].command, "policy")
    XCTAssertEqual(client.sentRuntimeControlCommandRequests[0].id, virtualMachine.id)
  }

  func testRuntimeControlPacingStoresDisplayPacingResponse() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let result = RuntimeControlCommandResult(
      vm: "Dev VM",
      kind: "apple-vz-display",
      socketPath: "/tmp/bvm-vz-test.sock",
      command: "pacing",
      response: GuestToolsCommandPayload(
        value: .object([
          "ok": .bool(true),
          "visibility": .string("background"),
          "display_fps_cap": .string("10"),
          "max_fps": .number("10"),
        ])
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      runtimeControlResult: .success(result)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didSend = await model.runtimeControlPacing(for: virtualMachine)

    XCTAssertTrue(didSend)
    XCTAssertEqual(model.runtimeControlResult(for: virtualMachine), result)
    XCTAssertNil(model.runtimeControlError(for: virtualMachine))
    XCTAssertEqual(client.sentRuntimeControlCommandRequests.count, 1)
    XCTAssertEqual(client.sentRuntimeControlCommandRequests[0].command, "pacing")
    XCTAssertEqual(client.sentRuntimeControlCommandRequests[0].id, virtualMachine.id)
  }

  func testRuntimeControlStopRefreshesRunnerStatusAfterDisplayStops() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let previousRunner = RunnerStatus(
      engine: "apple-vz",
      pid: 4242,
      command: ["lightvm-runner", "Dev VM", "--apple-vz-display"],
      logPath: "logs/lightvm.log",
      startedAtUnix: 1_710_000_100,
      dryRun: false,
      runtimeControl: RuntimeControlRunnerStatus(
        kind: "apple-vz-display",
        socketPath: "/tmp/bvm-vz-test.sock",
        commands: ["status", "stop", "policy", "pacing"]
      )
    )
    let result = RuntimeControlCommandResult(
      vm: "Dev VM",
      kind: "apple-vz-display",
      socketPath: "/tmp/bvm-vz-test.sock",
      command: "stop",
      response: GuestToolsCommandPayload(
        value: .object([
          "ok": .bool(true),
          "state": .string("stopping"),
        ])
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      prepareRunResult: .success(previousRunner),
      runnerStatusResult: .success(nil),
      runtimeControlResult: .success(result)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didPrepare = await model.prepareRun(for: virtualMachine)
    XCTAssertTrue(didPrepare)
    XCTAssertEqual(model.runnerStatus(for: virtualMachine), previousRunner)

    let didStop = await model.runtimeControlStopDisplay(for: virtualMachine)

    XCTAssertTrue(didStop)
    XCTAssertEqual(model.runtimeControlResult(for: virtualMachine), result)
    XCTAssertNil(model.runtimeControlError(for: virtualMachine))
    XCTAssertNil(model.runnerStatus(for: virtualMachine))
    XCTAssertNil(model.runnerStatusError(for: virtualMachine))
    XCTAssertEqual(client.sentRuntimeControlCommandRequests.count, 1)
    XCTAssertEqual(client.sentRuntimeControlCommandRequests[0].command, "stop")
    XCTAssertEqual(client.sentRuntimeControlCommandRequests[0].id, virtualMachine.id)
    XCTAssertEqual(client.inspectedRunnerStatusIDs, [virtualMachine.id])
  }

  func testLoadQemuLaunchPlanStoresSelectedVmPlan() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let plan = QemuLaunchPlan(
      program: "qemu-system-aarch64",
      args: ["-name", "Dev VM", "-netdev", "vmnet-host,id=net0"]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      qemuLaunchPlanResult: .success(plan)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.loadQemuLaunchPlan(for: virtualMachine)

    XCTAssertEqual(model.qemuLaunchPlan(for: virtualMachine), plan)
    XCTAssertEqual(
      model.qemuLaunchPlan(for: virtualMachine)?.commandLine,
      "qemu-system-aarch64 -name Dev VM -netdev vmnet-host,id=net0")
    XCTAssertNil(model.qemuLaunchPlanError(for: virtualMachine))
    XCTAssertNil(model.loadingQemuLaunchPlanID)
    XCTAssertEqual(client.inspectedQemuLaunchPlanIDs, [virtualMachine.id])
  }

  func testQemuLaunchPlanSummarizesNetworkModes() {
    XCTAssertEqual(
      QemuLaunchPlan(program: "qemu-system-aarch64", args: ["-netdev", "vmnet-host,id=net0"])
        .networkSummary,
      "Host-only"
    )
    XCTAssertEqual(
      QemuLaunchPlan(
        program: "qemu-system-aarch64",
        args: ["-netdev", "vmnet-bridged,id=net0,ifname=en0"]
      ).networkSummary,
      "Bridged"
    )
    XCTAssertEqual(
      QemuLaunchPlan(program: "qemu-system-aarch64", args: ["-netdev", "user,id=net0,restrict=on"])
        .networkSummary,
      "Isolated"
    )
    XCTAssertEqual(
      QemuLaunchPlan(
        program: "qemu-system-aarch64",
        args: ["-netdev", "user,id=net0,hostfwd=tcp::2222-:22"]
      ).networkSummary,
      "User NAT"
    )
  }

  func testQemuLaunchPlanDerivesVNCViewerEndpoint() {
    let defaultDisplay = QemuLaunchPlan(
      program: "qemu-system-aarch64",
      args: ["-display", "vnc=:0"]
    )
    let secondDisplay = QemuLaunchPlan(
      program: "qemu-system-aarch64",
      args: ["-display", "vnc=:2"]
    )
    let nonViewerDisplay = QemuLaunchPlan(
      program: "qemu-system-aarch64",
      args: ["-display", "default,show-cursor=on"]
    )

    XCTAssertEqual(defaultDisplay.viewerEndpoint?.absoluteString, "vnc://127.0.0.1:5900")
    XCTAssertEqual(secondDisplay.viewerEndpoint?.absoluteString, "vnc://127.0.0.1:5902")
    XCTAssertNil(nonViewerDisplay.viewerEndpoint)
  }

  func testQemuLaunchPlanRejectsMalformedVNCViewerEndpoints() {
    let missingDisplayValue = QemuLaunchPlan(
      program: "qemu-system-aarch64",
      args: ["-display"]
    )
    let negativeDisplay = QemuLaunchPlan(
      program: "qemu-system-aarch64",
      args: ["-display", "vnc=:-1"]
    )
    let nonNumericDisplay = QemuLaunchPlan(
      program: "qemu-system-aarch64",
      args: ["-display", "vnc=:abc"]
    )

    XCTAssertNil(missingDisplayValue.viewerEndpoint)
    XCTAssertNil(negativeDisplay.viewerEndpoint)
    XCTAssertNil(nonNumericDisplay.viewerEndpoint)
  }

  func testQemuLaunchPlanDerivesVNCViewerEndpointWithDisplayOptions() {
    let displayWithOptions = QemuLaunchPlan(
      program: "qemu-system-aarch64",
      args: ["-display", "vnc=:1,password=on"]
    )

    XCTAssertEqual(displayWithOptions.viewerEndpoint?.absoluteString, "vnc://127.0.0.1:5901")
  }

  func testLoadQemuLaunchPlanStoresError() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      qemuLaunchPlanResult: .failure(TestRefreshError.offline)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.loadQemuLaunchPlan(for: virtualMachine)

    XCTAssertNil(model.qemuLaunchPlan(for: virtualMachine))
    XCTAssertEqual(model.qemuLaunchPlanError(for: virtualMachine), "Offline")
    XCTAssertEqual(model.alertMessage, "Offline")
    XCTAssertNil(model.loadingQemuLaunchPlanID)
    XCTAssertEqual(client.inspectedQemuLaunchPlanIDs, [virtualMachine.id])
  }

  func testLoadSnapshotPreflightStatusStoresSelectedVmStatus() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let status = SnapshotPreflightStatus(
      vm: "Dev VM",
      consistency: .applicationConsistent,
      backendFreezeThawSupported: false,
      guestToolsConnected: true,
      capabilities: ["guest-tools-heartbeat", "filesystem-freeze-preflight"],
      ready: false,
      blockers: [
        SnapshotPreflightBlocker(
          code: "backend-freeze-thaw-unavailable",
          message:
            "Freeze/thaw orchestration requires the bridgevmd-owned running backend; this offline preflight cannot drive the guest agent.",
          path: nil
        )
      ],
      checkedAtUnix: 1_710_000_200
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      snapshotPreflightStatusResult: .success(status)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.loadSnapshotPreflightStatus(for: virtualMachine)

    XCTAssertEqual(model.snapshotPreflightStatus(for: virtualMachine), status)
    XCTAssertNil(model.snapshotPreflightStatusError(for: virtualMachine))
    XCTAssertEqual(
      model.snapshotPreflightStatus(for: virtualMachine)?.readinessTitle, "Scaffold only")
    XCTAssertEqual(client.inspectedSnapshotPreflightStatusIDs, [virtualMachine.id])
  }

  func testLoadSnapshotsStoresSelectedVmSnapshots() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let snapshots = [
      VMSnapshot(
        name: "before-upgrade",
        kind: .disk,
        createdAtUnix: 1_710_000_100,
        vmState: .stopped
      ),
      VMSnapshot(
        name: "paused-state",
        kind: .suspend,
        createdAtUnix: 1_710_000_200,
        vmState: .suspended
      ),
    ]
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      snapshotsResult: .success(snapshots)
    )
    let model = DashboardViewModel(client: client)

    await model.loadSnapshots(for: virtualMachine)

    XCTAssertEqual(model.snapshots(for: virtualMachine), snapshots)
    XCTAssertNil(model.snapshotError(for: virtualMachine))
    XCTAssertEqual(client.listedSnapshotIDs, [virtualMachine.id])
    XCTAssertNil(model.loadingSnapshotsID)
  }

  func testLoadSnapshotsStoresError() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      snapshotsResult: .failure(TestRefreshError.offline)
    )
    let model = DashboardViewModel(client: client)

    await model.loadSnapshots(for: virtualMachine)

    XCTAssertTrue(model.snapshots(for: virtualMachine).isEmpty)
    XCTAssertEqual(model.snapshotError(for: virtualMachine), "Offline")
    XCTAssertEqual(model.alertMessage, "Offline")
    XCTAssertEqual(client.listedSnapshotIDs, [virtualMachine.id])
    XCTAssertNil(model.loadingSnapshotsID)
  }

  func testLoadSnapshotChainStoresSelectedVmChain() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let chain = VMSnapshotChain(
      activeDisk: VMActiveDisk(
        source: "snapshot-overlay",
        snapshot: "before-upgrade",
        path: "disks/snapshots/before-upgrade.qcow2",
        format: "qcow2",
        exists: true,
        activatedAtUnix: 1_710_000_250
      ),
      disks: [
        VMSnapshotDisk(
          snapshot: "before-upgrade",
          overlayPath: "disks/snapshots/before-upgrade.qcow2",
          overlayFormat: "qcow2",
          overlayExists: true,
          backingPath: "disks/root.qcow2",
          backingFormat: "qcow2",
          backingExists: true,
          createCommand: ["qemu-img", "create", "-f", "qcow2"],
          preparedAtUnix: 1_710_000_200
        )
      ]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      snapshotChainResult: .success(chain)
    )
    let model = DashboardViewModel(client: client)

    await model.loadSnapshotChain(for: virtualMachine)

    XCTAssertEqual(model.snapshotChain(for: virtualMachine), chain)
    XCTAssertNil(model.snapshotChainError(for: virtualMachine))
    XCTAssertEqual(model.snapshotChain(for: virtualMachine)?.readinessTitle, "Chain ready")
    XCTAssertEqual(client.inspectedSnapshotChainIDs, [virtualMachine.id])
    XCTAssertNil(model.loadingSnapshotChainID)
  }

  func testCreateSnapshotDiskRejectsEmptyName() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: client)

    let created = await model.createSnapshotDisk(named: "   ", for: virtualMachine)

    XCTAssertFalse(created)
    XCTAssertTrue(client.createdSnapshotDiskRequests.isEmpty)
    XCTAssertEqual(model.alertMessage, "Enter a snapshot name before creating a disk overlay.")
    XCTAssertNil(model.creatingSnapshotDiskID)
  }

  func testCreateSnapshotRejectsEmptyName() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: client)

    let created = await model.createSnapshot(
      named: "   ",
      kind: .disk,
      for: virtualMachine
    )

    XCTAssertFalse(created)
    XCTAssertTrue(client.createdSnapshotRequests.isEmpty)
    XCTAssertEqual(model.alertMessage, "Enter a snapshot name before creating metadata.")
    XCTAssertNil(model.creatingSnapshotID)
  }

  func testCreateSnapshotStoresMetadataAndRefreshesSnapshotsAndChain() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let snapshot = VMSnapshot(
      name: "before-upgrade",
      kind: .applicationConsistent,
      createdAtUnix: 1_710_000_300,
      vmState: .running
    )
    let activeDisk = VMActiveDisk(
      source: "primary",
      snapshot: nil,
      path: "disks/root.qcow2",
      format: "qcow2",
      exists: true,
      activatedAtUnix: 1_710_000_300
    )
    let chain = VMSnapshotChain(activeDisk: activeDisk, disks: [])
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      snapshotsResult: .success([snapshot]),
      snapshotChainResult: .success(chain),
      snapshotCreationResult: .success(snapshot)
    )
    let model = DashboardViewModel(client: client)

    let created = await model.createSnapshot(
      named: " before-upgrade ",
      kind: .applicationConsistent,
      for: virtualMachine
    )

    XCTAssertTrue(created)
    XCTAssertEqual(model.snapshotCreation(for: virtualMachine), snapshot)
    XCTAssertNil(model.snapshotCreationError(for: virtualMachine))
    XCTAssertEqual(model.snapshots(for: virtualMachine), [snapshot])
    XCTAssertEqual(model.snapshotChain(for: virtualMachine), chain)
    XCTAssertEqual(client.createdSnapshotRequests.count, 1)
    XCTAssertEqual(client.createdSnapshotRequests[0].snapshotName, "before-upgrade")
    XCTAssertEqual(client.createdSnapshotRequests[0].kind, .applicationConsistent)
    XCTAssertEqual(client.createdSnapshotRequests[0].id, virtualMachine.id)
    XCTAssertEqual(client.listedSnapshotIDs, [virtualMachine.id])
    XCTAssertEqual(client.inspectedSnapshotChainIDs, [virtualMachine.id])
    XCTAssertEqual(
      model.alertMessage,
      "Snapshot 'before-upgrade' application-consistent metadata created."
    )
    XCTAssertNil(model.creatingSnapshotID)
  }

  func testCreateSnapshotStoresError() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      snapshotCreationResult: .failure(TestRefreshError.offline)
    )
    let model = DashboardViewModel(client: client)

    let created = await model.createSnapshot(
      named: "before-upgrade",
      kind: .disk,
      for: virtualMachine
    )

    XCTAssertFalse(created)
    XCTAssertNil(model.snapshotCreation(for: virtualMachine))
    XCTAssertEqual(model.snapshotCreationError(for: virtualMachine), "Offline")
    XCTAssertEqual(model.alertMessage, "Offline")
    XCTAssertEqual(client.createdSnapshotRequests.count, 1)
    XCTAssertEqual(client.createdSnapshotRequests[0].snapshotName, "before-upgrade")
    XCTAssertEqual(client.createdSnapshotRequests[0].kind, .disk)
    XCTAssertEqual(client.createdSnapshotRequests[0].id, virtualMachine.id)
    XCTAssertTrue(client.listedSnapshotIDs.isEmpty)
    XCTAssertTrue(client.inspectedSnapshotChainIDs.isEmpty)
    XCTAssertNil(model.creatingSnapshotID)
  }

  func testCreateSnapshotDiskStoresResultAndRefreshesSnapshotChain() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let disk = VMSnapshotDisk(
      snapshot: "before-upgrade",
      overlayPath: "disks/snapshots/before-upgrade.qcow2",
      overlayFormat: "qcow2",
      overlayExists: true,
      backingPath: "disks/root.qcow2",
      backingFormat: "qcow2",
      backingExists: true,
      createCommand: [
        "qemu-img", "create", "-f", "qcow2", "-b", "disks/root.qcow2",
        "disks/snapshots/before-upgrade.qcow2",
      ],
      preparedAtUnix: 1_710_000_200
    )
    let creation = VMSnapshotDiskCreation(
      snapshot: "before-upgrade",
      disk: disk,
      command: disk.createCommand,
      executed: true,
      exitStatus: "exit status: 0",
      stdout: "",
      stderr: "",
      createdAtUnix: 1_710_000_300
    )
    let activeDisk = VMActiveDisk(
      source: "snapshot-overlay",
      snapshot: "before-upgrade",
      path: "disks/snapshots/before-upgrade.qcow2",
      format: "qcow2",
      exists: true,
      activatedAtUnix: 1_710_000_300
    )
    let chain = VMSnapshotChain(activeDisk: activeDisk, disks: [disk])
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      snapshotChainResult: .success(chain),
      snapshotDiskCreationResult: .success(creation)
    )
    let model = DashboardViewModel(client: client)

    let created = await model.createSnapshotDisk(named: " before-upgrade ", for: virtualMachine)

    XCTAssertTrue(created)
    XCTAssertEqual(model.snapshotDiskCreation(for: virtualMachine), creation)
    XCTAssertNil(model.snapshotDiskCreationError(for: virtualMachine))
    XCTAssertEqual(model.snapshotChain(for: virtualMachine), chain)
    XCTAssertEqual(client.createdSnapshotDiskRequests.count, 1)
    XCTAssertEqual(client.createdSnapshotDiskRequests[0].snapshotName, "before-upgrade")
    XCTAssertEqual(client.createdSnapshotDiskRequests[0].id, virtualMachine.id)
    XCTAssertEqual(client.inspectedSnapshotChainIDs, [virtualMachine.id])
    XCTAssertEqual(
      model.alertMessage,
      "Snapshot disk 'before-upgrade' create command finished with exit status: 0."
    )
    XCTAssertNil(model.creatingSnapshotDiskID)
  }

  func testCreateSnapshotDiskStoresError() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      snapshotDiskCreationResult: .failure(TestRefreshError.offline)
    )
    let model = DashboardViewModel(client: client)

    let created = await model.createSnapshotDisk(named: "before-upgrade", for: virtualMachine)

    XCTAssertFalse(created)
    XCTAssertNil(model.snapshotDiskCreation(for: virtualMachine))
    XCTAssertEqual(model.snapshotDiskCreationError(for: virtualMachine), "Offline")
    XCTAssertEqual(model.alertMessage, "Offline")
    XCTAssertEqual(client.createdSnapshotDiskRequests.count, 1)
    XCTAssertEqual(client.createdSnapshotDiskRequests[0].snapshotName, "before-upgrade")
    XCTAssertEqual(client.createdSnapshotDiskRequests[0].id, virtualMachine.id)
    XCTAssertTrue(client.inspectedSnapshotChainIDs.isEmpty)
    XCTAssertNil(model.creatingSnapshotDiskID)
  }

  func testSnapshotMetadataMutationsRejectOverlappingActionsForSameVM() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )

    let createClient = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      snapshotDiskCreationDelayNanos: 50_000_000
    )
    let createModel = DashboardViewModel(client: createClient)
    let activeCreate = Task {
      await createModel.createSnapshotDisk(named: "before-upgrade", for: virtualMachine)
    }
    await Task.yield()

    XCTAssertEqual(createModel.creatingSnapshotDiskID, virtualMachine.id)
    let createBlockedRestore = await createModel.restoreSnapshot(
      named: "before-upgrade",
      for: virtualMachine
    )
    let createBlockedMetadata = await createModel.createSnapshot(
      named: "before-upgrade",
      kind: .disk,
      for: virtualMachine
    )
    let createBlockedApplicationConsistent = await createModel.executeApplicationConsistentSnapshot(
      named: "before-upgrade",
      for: virtualMachine
    )
    XCTAssertFalse(createBlockedRestore)
    XCTAssertFalse(createBlockedMetadata)
    XCTAssertFalse(createBlockedApplicationConsistent)
    XCTAssertTrue(createClient.restoredSnapshotRequests.isEmpty)
    XCTAssertTrue(createClient.createdSnapshotRequests.isEmpty)
    XCTAssertTrue(createClient.executedApplicationConsistentSnapshotRequests.isEmpty)
    _ = await activeCreate.value

    let metadataClient = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      snapshotCreationDelayNanos: 50_000_000
    )
    let metadataModel = DashboardViewModel(client: metadataClient)
    let activeMetadataCreate = Task {
      await metadataModel.createSnapshot(named: "before-upgrade", kind: .disk, for: virtualMachine)
    }
    await Task.yield()

    XCTAssertEqual(metadataModel.creatingSnapshotID, virtualMachine.id)
    let metadataBlockedDiskCreate =
      await metadataModel.createSnapshotDisk(named: "before-upgrade", for: virtualMachine)
    let metadataBlockedRestore =
      await metadataModel.restoreSnapshot(named: "before-upgrade", for: virtualMachine)
    let metadataBlockedApplicationConsistent =
      await metadataModel.executeApplicationConsistentSnapshot(
        named: "before-upgrade",
        for: virtualMachine
      )
    XCTAssertFalse(metadataBlockedDiskCreate)
    XCTAssertFalse(metadataBlockedRestore)
    XCTAssertFalse(metadataBlockedApplicationConsistent)
    XCTAssertTrue(metadataClient.createdSnapshotDiskRequests.isEmpty)
    XCTAssertTrue(metadataClient.restoredSnapshotRequests.isEmpty)
    XCTAssertTrue(metadataClient.executedApplicationConsistentSnapshotRequests.isEmpty)
    _ = await activeMetadataCreate.value

    let restoreClient = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      snapshotRestoreDelayNanos: 50_000_000
    )
    let restoreModel = DashboardViewModel(client: restoreClient)
    let activeRestore = Task {
      await restoreModel.restoreSnapshot(named: "before-upgrade", for: virtualMachine)
    }
    await Task.yield()

    XCTAssertEqual(restoreModel.restoringSnapshotID, virtualMachine.id)
    let restoreBlockedCreate = await restoreModel.createSnapshotDisk(
      named: "before-upgrade",
      for: virtualMachine
    )
    let restoreBlockedMetadataCreate = await restoreModel.createSnapshot(
      named: "before-upgrade",
      kind: .disk,
      for: virtualMachine
    )
    let restoreBlockedApplicationConsistent =
      await restoreModel.executeApplicationConsistentSnapshot(
        named: "before-upgrade",
        for: virtualMachine
      )
    XCTAssertFalse(restoreBlockedCreate)
    XCTAssertFalse(restoreBlockedMetadataCreate)
    XCTAssertFalse(restoreBlockedApplicationConsistent)
    XCTAssertTrue(restoreClient.createdSnapshotDiskRequests.isEmpty)
    XCTAssertTrue(restoreClient.createdSnapshotRequests.isEmpty)
    XCTAssertTrue(restoreClient.executedApplicationConsistentSnapshotRequests.isEmpty)
    _ = await activeRestore.value

    let applicationConsistentClient = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      applicationConsistentSnapshotExecutionDelayNanos: 50_000_000
    )
    let applicationConsistentModel = DashboardViewModel(client: applicationConsistentClient)
    let activeApplicationConsistentSnapshot = Task {
      await applicationConsistentModel.executeApplicationConsistentSnapshot(
        named: "before-upgrade",
        for: virtualMachine
      )
    }
    await Task.yield()

    XCTAssertEqual(
      applicationConsistentModel.executingApplicationConsistentSnapshotID,
      virtualMachine.id
    )
    let applicationConsistentBlockedCreate =
      await applicationConsistentModel.createSnapshotDisk(
        named: "before-upgrade",
        for: virtualMachine
      )
    let applicationConsistentBlockedMetadataCreate =
      await applicationConsistentModel.createSnapshot(
        named: "before-upgrade",
        kind: .disk,
        for: virtualMachine
      )
    let applicationConsistentBlockedRestore =
      await applicationConsistentModel.restoreSnapshot(
        named: "before-upgrade",
        for: virtualMachine
      )
    XCTAssertFalse(applicationConsistentBlockedCreate)
    XCTAssertFalse(applicationConsistentBlockedMetadataCreate)
    XCTAssertFalse(applicationConsistentBlockedRestore)
    XCTAssertTrue(applicationConsistentClient.createdSnapshotDiskRequests.isEmpty)
    XCTAssertTrue(applicationConsistentClient.createdSnapshotRequests.isEmpty)
    XCTAssertTrue(applicationConsistentClient.restoredSnapshotRequests.isEmpty)
    _ = await activeApplicationConsistentSnapshot.value
  }

  func testVerifyActiveDiskStoresVerificationMetadata() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let verification = VMDiskVerification(
      activeDisk: VMActiveDisk(
        source: "primary",
        snapshot: nil,
        path: "disks/root.qcow2",
        format: "qcow2",
        exists: true,
        activatedAtUnix: 1_710_000_100
      ),
      command: ["qemu-img", "check", "--output=json", "disks/root.qcow2"],
      exitStatus: "exit status: 0",
      report: "{\n  \"check-errors\" : 0\n}",
      reportValue: .object(["check-errors": .int(0)]),
      stdout: "{\"check-errors\":0}",
      stderr: "",
      verifyDurationMicroseconds: 42,
      verifiedAtUnix: 1_710_000_400
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      diskVerificationResult: .success(verification)
    )
    let model = DashboardViewModel(client: client)

    let verified = await model.verifyActiveDisk(for: virtualMachine)

    XCTAssertTrue(verified)
    XCTAssertEqual(model.diskVerification(for: virtualMachine), verification)
    XCTAssertNil(model.diskVerificationError(for: virtualMachine))
    XCTAssertEqual(client.verifiedDiskIDs, [virtualMachine.id])
    XCTAssertEqual(model.alertMessage, "Active disk verified with status exit status: 0.")
    XCTAssertNil(model.verifyingDiskID)
  }

  func testPreparePrimaryDiskStoresPreparationMetadata() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let preparation = DiskPreparation(
      path: "disks/root.qcow2",
      format: "qcow2",
      size: "64G",
      sizeBytes: 68_719_476_736,
      exists: true,
      created: false,
      createCommand: nil,
      preparedAtUnix: 1_710_000_100
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      diskPreparationResult: .success(preparation)
    )
    let model = DashboardViewModel(client: client)

    let prepared = await model.preparePrimaryDisk(for: virtualMachine)

    XCTAssertTrue(prepared)
    XCTAssertEqual(model.diskPreparation(for: virtualMachine), preparation)
    XCTAssertNil(model.diskPreparationError(for: virtualMachine))
    XCTAssertEqual(client.preparedDiskIDs, [virtualMachine.id])
    XCTAssertEqual(model.alertMessage, "Primary disk metadata prepared for disks/root.qcow2.")
    XCTAssertNil(model.preparingDiskID)
  }

  func testCreatePrimaryDiskStoresCreationAndPreparationMetadata() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let preparation = DiskPreparation(
      path: "disks/root.qcow2",
      format: "qcow2",
      size: "64G",
      sizeBytes: 68_719_476_736,
      exists: true,
      created: true,
      createCommand: ["qemu-img", "create", "-f", "qcow2", "disks/root.qcow2", "64G"],
      preparedAtUnix: 1_710_000_100
    )
    let creation = VMDiskCreation(
      preparation: preparation,
      command: ["qemu-img", "create", "-f", "qcow2", "disks/root.qcow2", "64G"],
      executed: true,
      exitStatus: "exit status: 0",
      stdout: "",
      stderr: "",
      createdAtUnix: 1_710_000_200
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      diskCreationResult: .success(creation)
    )
    let model = DashboardViewModel(client: client)

    let created = await model.createPrimaryDisk(for: virtualMachine)

    XCTAssertTrue(created)
    XCTAssertEqual(model.diskCreation(for: virtualMachine), creation)
    XCTAssertEqual(model.diskPreparation(for: virtualMachine), preparation)
    XCTAssertNil(model.diskCreationError(for: virtualMachine))
    XCTAssertEqual(client.createdDiskIDs, [virtualMachine.id])
    XCTAssertEqual(
      model.alertMessage,
      "Primary disk create command finished with exit status: 0."
    )
    XCTAssertNil(model.creatingDiskID)
  }

  func testInspectPrimaryDiskStoresInspectionAndPreparationMetadata() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let preparation = DiskPreparation(
      path: "disks/root.qcow2",
      format: "qcow2",
      size: "64G",
      sizeBytes: 68_719_476_736,
      exists: true,
      created: false,
      createCommand: nil,
      preparedAtUnix: 1_710_000_100
    )
    let inspection = VMDiskInspection(
      preparation: preparation,
      command: ["qemu-img", "info", "--output=json", "disks/root.qcow2"],
      exitStatus: "exit status: 0",
      info: "{\n  \"format\" : \"qcow2\"\n}",
      infoValue: .object(["format": .string("qcow2")]),
      stdout: "{\"format\":\"qcow2\"}",
      stderr: "",
      inspectDurationMicroseconds: 64,
      inspectedAtUnix: 1_710_000_300
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      diskInspectionResult: .success(inspection)
    )
    let model = DashboardViewModel(client: client)

    let inspected = await model.inspectPrimaryDisk(for: virtualMachine)

    XCTAssertTrue(inspected)
    XCTAssertEqual(model.diskInspection(for: virtualMachine), inspection)
    XCTAssertEqual(model.diskPreparation(for: virtualMachine), preparation)
    XCTAssertNil(model.diskInspectionError(for: virtualMachine))
    XCTAssertEqual(client.inspectedDiskIDs, [virtualMachine.id])
    XCTAssertEqual(model.alertMessage, "Primary disk inspected with status exit status: 0.")
    XCTAssertNil(model.inspectingDiskID)
  }

  func testCompactActiveDiskStoresCompactionAndRefreshesChain() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let activeDisk = VMActiveDisk(
      source: "primary",
      snapshot: nil,
      path: "disks/root.qcow2",
      format: "qcow2",
      exists: true,
      activatedAtUnix: 1_710_000_100
    )
    let compaction = VMDiskCompaction(
      preparation: DiskPreparation(
        path: "disks/root.qcow2",
        format: "qcow2",
        size: "64G",
        sizeBytes: 1_024,
        exists: true,
        created: false,
        createCommand: nil,
        preparedAtUnix: 1_710_000_100
      ),
      activeDisk: activeDisk,
      command: ["qemu-img", "convert", "-O", "qcow2", "disks/root.qcow2"],
      tempPath: "disks/root.compact.tmp",
      backupPath: "disks/root.precompact-1710000500.qcow2",
      exitStatus: "exit status: 0",
      stdout: "",
      stderr: "",
      originalSizeBytes: 1_024,
      compactedSizeBytes: 512,
      compactDurationMicroseconds: 84,
      compactedAtUnix: 1_710_000_500
    )
    let chain = VMSnapshotChain(activeDisk: activeDisk, disks: [])
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      snapshotChainResult: .success(chain),
      diskCompactionResult: .success(compaction)
    )
    let model = DashboardViewModel(client: client)

    let compacted = await model.compactActiveDisk(for: virtualMachine)

    XCTAssertTrue(compacted)
    XCTAssertEqual(model.diskCompaction(for: virtualMachine), compaction)
    XCTAssertNil(model.diskCompactionError(for: virtualMachine))
    XCTAssertEqual(model.snapshotChain(for: virtualMachine), chain)
    XCTAssertEqual(client.compactedDiskIDs, [virtualMachine.id])
    XCTAssertEqual(client.inspectedSnapshotChainIDs, [virtualMachine.id])
    XCTAssertEqual(
      model.alertMessage,
      "Active disk compacted; previous image kept at disks/root.precompact-1710000500.qcow2."
    )
    XCTAssertNil(model.compactingDiskID)
  }

  func testRepairMetadataStoresRepairAndRefreshesMetadataViews() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let activeDisk = VMActiveDisk(
      source: "primary",
      snapshot: nil,
      path: "disks/root.qcow2",
      format: "qcow2",
      exists: true,
      activatedAtUnix: 1_710_000_100
    )
    let chain = VMSnapshotChain(activeDisk: activeDisk, disks: [])
    let snapshots = [
      VMSnapshot(
        name: "before-upgrade",
        kind: .disk,
        createdAtUnix: 1_710_000_200,
        vmState: .stopped
      )
    ]
    let repair = VMMetadataRepair(
      vm: "Dev VM",
      bundle: "/tmp/store/vms/dev-vm.vmbridge",
      repaired: true,
      actions: [
        VMMetadataRepairAction(
          action: "repaired",
          path: "metadata/active-disk.json",
          detail: "Recreated active disk metadata."
        )
      ],
      repairedAtUnix: 1_710_000_600
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      snapshotsResult: .success(snapshots),
      snapshotChainResult: .success(chain),
      metadataRepairResult: .success(repair)
    )
    let model = DashboardViewModel(client: client)

    let repaired = await model.repairMetadata(for: virtualMachine)

    XCTAssertTrue(repaired)
    XCTAssertEqual(model.metadataRepair(for: virtualMachine), repair)
    XCTAssertNil(model.metadataRepairError(for: virtualMachine))
    XCTAssertEqual(model.snapshots(for: virtualMachine), snapshots)
    XCTAssertEqual(model.snapshotChain(for: virtualMachine), chain)
    XCTAssertEqual(client.repairedMetadataIDs, [virtualMachine.id])
    XCTAssertEqual(client.listedSnapshotIDs, [virtualMachine.id])
    XCTAssertEqual(client.inspectedSnapshotChainIDs, [virtualMachine.id])
    XCTAssertEqual(model.alertMessage, "Metadata repaired with 1 action(s).")
    XCTAssertNil(model.repairingMetadataID)
  }

  func testRepairMetadataStoresNoopResult() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let repair = VMMetadataRepair(
      vm: "Dev VM",
      bundle: "/tmp/store/vms/dev-vm.vmbridge",
      repaired: false,
      actions: [],
      repairedAtUnix: 1_710_000_600
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      metadataRepairResult: .success(repair)
    )
    let model = DashboardViewModel(client: client)

    let repaired = await model.repairMetadata(for: virtualMachine)

    XCTAssertTrue(repaired)
    XCTAssertEqual(model.metadataRepair(for: virtualMachine), repair)
    XCTAssertNil(model.metadataRepairError(for: virtualMachine))
    XCTAssertEqual(
      model.alertMessage, "Metadata repair completed; no metadata repairs were needed.")
    XCTAssertNil(model.repairingMetadataID)
  }

  func testRepairMetadataStoresError() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      metadataRepairResult: .failure(VirtualMachineClientError.daemonResponseInvalid)
    )
    let model = DashboardViewModel(client: client)

    let repaired = await model.repairMetadata(for: virtualMachine)

    XCTAssertFalse(repaired)
    XCTAssertNil(model.metadataRepair(for: virtualMachine))
    XCTAssertEqual(
      model.metadataRepairError(for: virtualMachine),
      "The daemon returned a response that the app could not understand."
    )
    XCTAssertEqual(client.repairedMetadataIDs, [virtualMachine.id])
    XCTAssertNil(model.repairingMetadataID)
  }

  func testCheckManifestMigrationStoresDryRunResult() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let migration = VMManifestMigration(
      vm: "Dev VM",
      bundle: "/tmp/store/vms/dev-vm.vmbridge",
      manifestPath: "/tmp/store/vms/dev-vm.vmbridge/manifest.yaml",
      dryRun: true,
      migrated: true,
      fromSchema: "bridgevm.io/v0",
      toSchema: "bridgevm.io/v1",
      actions: ["would add metadata envelope", "would write migration receipt"],
      backupPath: nil,
      receiptPath: nil,
      migratedAtUnix: 1_710_000_700
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      manifestMigrationResult: .success(migration)
    )
    let model = DashboardViewModel(client: client)

    let checked = await model.checkManifestMigration(for: virtualMachine)

    XCTAssertTrue(checked)
    XCTAssertEqual(model.manifestMigration(for: virtualMachine), migration)
    XCTAssertNil(model.manifestMigrationError(for: virtualMachine))
    XCTAssertEqual(client.migratedManifestRequests.map(\.id), [virtualMachine.id])
    XCTAssertEqual(client.migratedManifestRequests.map(\.dryRun), [true])
    XCTAssertEqual(model.alertMessage, "Manifest migration dry run found 2 action(s).")
    XCTAssertNil(model.checkingManifestMigrationID)
  }

  func testCheckManifestMigrationStoresCurrentManifestResult() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let migration = VMManifestMigration(
      vm: "Dev VM",
      bundle: "/tmp/store/vms/dev-vm.vmbridge",
      manifestPath: "/tmp/store/vms/dev-vm.vmbridge/manifest.yaml",
      dryRun: true,
      migrated: false,
      fromSchema: "bridgevm.io/v1",
      toSchema: "bridgevm.io/v1",
      actions: ["validated current manifest schema"],
      backupPath: nil,
      receiptPath: nil,
      migratedAtUnix: 1_710_000_700
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      manifestMigrationResult: .success(migration)
    )
    let model = DashboardViewModel(client: client)

    let checked = await model.checkManifestMigration(for: virtualMachine)

    XCTAssertTrue(checked)
    XCTAssertEqual(model.manifestMigration(for: virtualMachine), migration)
    XCTAssertNil(model.manifestMigrationError(for: virtualMachine))
    XCTAssertEqual(model.alertMessage, "Manifest migration dry run completed; manifest is current.")
    XCTAssertNil(model.checkingManifestMigrationID)
  }

  func testCheckManifestMigrationStoresError() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      manifestMigrationResult: .failure(VirtualMachineClientError.daemonResponseInvalid)
    )
    let model = DashboardViewModel(client: client)

    let checked = await model.checkManifestMigration(for: virtualMachine)

    XCTAssertFalse(checked)
    XCTAssertNil(model.manifestMigration(for: virtualMachine))
    XCTAssertEqual(
      model.manifestMigrationError(for: virtualMachine),
      "The daemon returned a response that the app could not understand."
    )
    XCTAssertEqual(client.migratedManifestRequests.map(\.id), [virtualMachine.id])
    XCTAssertEqual(client.migratedManifestRequests.map(\.dryRun), [true])
    XCTAssertNil(model.checkingManifestMigrationID)
  }

  func testRestoreSnapshotStoresResultAndRefreshesSnapshots() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let restore = SnapshotRestoreResult(
      snapshot: "before-upgrade",
      restoredAtUnix: 1_710_000_300,
      restoredState: .stopped,
      activeDisk: SnapshotActiveDisk(
        source: "snapshot-backing",
        snapshot: "before-upgrade",
        path: "disks/root.qcow2",
        format: "qcow2",
        exists: true,
        activatedAtUnix: 1_710_000_300
      ),
      suspendImage: nil
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      snapshotsResult: .success([]),
      snapshotChainResult: .success(
        VMSnapshotChain(
          activeDisk: VMActiveDisk(
            source: "snapshot-backing",
            snapshot: "before-upgrade",
            path: "disks/root.qcow2",
            format: "qcow2",
            exists: true,
            activatedAtUnix: 1_710_000_300
          ),
          disks: []
        )
      ),
      snapshotRestoreResult: .success(restore)
    )
    let model = DashboardViewModel(client: client)

    let restored = await model.restoreSnapshot(named: " before-upgrade ", for: virtualMachine)

    XCTAssertTrue(restored)
    XCTAssertEqual(model.snapshotRestoreResult(for: virtualMachine), restore)
    XCTAssertNil(model.snapshotRestoreError(for: virtualMachine))
    XCTAssertEqual(client.restoredSnapshotRequests.count, 1)
    XCTAssertEqual(client.restoredSnapshotRequests[0].snapshotName, "before-upgrade")
    XCTAssertEqual(client.listedSnapshotIDs, [virtualMachine.id])
    XCTAssertEqual(client.inspectedSnapshotChainIDs, [virtualMachine.id])
    XCTAssertEqual(model.alertMessage, "Snapshot 'before-upgrade' metadata restored.")
    XCTAssertNil(model.restoringSnapshotID)
  }

  func testRestoreSnapshotRejectsEmptyName() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: client)

    let restored = await model.restoreSnapshot(named: " ", for: virtualMachine)

    XCTAssertFalse(restored)
    XCTAssertEqual(model.alertMessage, "Select a snapshot to restore.")
    XCTAssertTrue(client.restoredSnapshotRequests.isEmpty)
    XCTAssertNil(model.restoringSnapshotID)
  }

  func testExecuteApplicationConsistentSnapshotStoresExecutionResult() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let execution = ApplicationConsistentSnapshotExecution(
      vm: "Dev VM",
      snapshot: "before-upgrade",
      freezeRequestID: "application-consistent-snapshot:before-upgrade:freeze",
      thawRequestID: "application-consistent-snapshot:before-upgrade:thaw",
      pendingCommandsAfterFreeze: 1,
      pendingCommandsAfterThaw: 2,
      snapshotCreatedAtUnix: 1_710_000_300,
      freezeResult: ApplicationConsistentSnapshotCommandResult(
        requestID: "application-consistent-snapshot:before-upgrade:freeze",
        capability: "fs-freeze",
        ok: true,
        errorCode: nil,
        message: "freeze scaffold acknowledged",
        completedAtUnix: 1_710_000_280
      ),
      thawResult: ApplicationConsistentSnapshotCommandResult(
        requestID: "application-consistent-snapshot:before-upgrade:thaw",
        capability: "fs-thaw",
        ok: true,
        errorCode: nil,
        message: "thaw scaffold acknowledged",
        completedAtUnix: 1_710_000_290
      ),
      preflightReady: true,
      note: "scaffold boundary"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      applicationConsistentSnapshotExecutionResult: .success(execution)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didExecute = await model.executeApplicationConsistentSnapshot(
      named: "  before-upgrade  ",
      freezeTimeoutMillis: 5_000,
      for: virtualMachine
    )

    XCTAssertTrue(didExecute)
    XCTAssertEqual(
      model.applicationConsistentSnapshotExecution(for: virtualMachine), execution)
    XCTAssertNil(model.applicationConsistentSnapshotExecutionError(for: virtualMachine))
    XCTAssertEqual(client.executedApplicationConsistentSnapshotRequests.count, 1)
    XCTAssertEqual(
      client.executedApplicationConsistentSnapshotRequests[0].snapshotName, "before-upgrade")
    XCTAssertEqual(
      client.executedApplicationConsistentSnapshotRequests[0].freezeTimeoutMillis, 5_000)
    XCTAssertEqual(client.executedApplicationConsistentSnapshotRequests[0].id, virtualMachine.id)
    XCTAssertEqual(
      model.alertMessage,
      "Application-consistent snapshot 'before-upgrade' executed for Dev VM."
    )
  }

  func testReapplyRuntimeResourcesStoresPolicy() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let policy = RuntimeResourcePolicy(
      vm: "dev",
      mode: "fast",
      profile: "automatic",
      visibility: .background,
      state: "running",
      onBattery: false,
      memory: "2048",
      cpu: "1",
      displayFPSCap: "10",
      rationale: "Battery or background throttling active.",
      liveApplied: false,
      runtimeControlAcknowledged: false,
      liveApplyBlockers: [
        RuntimeResourcePolicyBlocker(
          code: "runtime-control-unavailable",
          message: "Live Apple VZ CPU/RAM hot-apply is not implemented yet; the policy is available to display pacing and runtime policy IPC consumers."
        )
      ],
      updatedAtUnix: 1_710_000_500
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      runtimeResourcePolicyResult: .success(policy)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didReapply = await model.reapplyRuntimeResources(
      visibility: .background,
      for: virtualMachine
    )

    XCTAssertTrue(didReapply)
    XCTAssertEqual(model.runtimeResourcePolicy(for: virtualMachine), policy)
    XCTAssertNil(model.runtimeResourcePolicyError(for: virtualMachine))
    XCTAssertEqual(client.reappliedRuntimeResourceRequests.count, 1)
    XCTAssertEqual(client.reappliedRuntimeResourceRequests[0].visibility, .background)
    XCTAssertEqual(client.reappliedRuntimeResourceRequests[0].id, virtualMachine.id)
    let expectedAlert =
      "Runtime resource policy recorded for dev; live apply blocked: runtime-control-unavailable: Live Apple VZ CPU/RAM hot-apply is not implemented yet; the policy is available to display pacing and runtime policy IPC consumers."
    XCTAssertEqual(
      model.alertMessage,
      expectedAlert
    )
  }

  func testReapplyRuntimeResourcesReportsDisplayHelperAcknowledgement() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let policy = RuntimeResourcePolicy(
      vm: "dev",
      mode: "fast",
      profile: "automatic",
      visibility: .foreground,
      state: "running",
      onBattery: false,
      memory: "4096",
      cpu: "2",
      displayFPSCap: "adaptive",
      rationale: "Foreground display active.",
      liveApplied: false,
      runtimeControlAcknowledged: true,
      liveApplyBlockers: [
        RuntimeResourcePolicyBlocker(
          code: "runtime-control-unavailable",
          message: "Live Apple VZ CPU/RAM hot-apply is not implemented yet; the policy is available to display pacing and runtime policy IPC consumers."
        )
      ],
      updatedAtUnix: 1_710_000_500
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      runtimeResourcePolicyResult: .success(policy)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didReapply = await model.reapplyRuntimeResources(
      visibility: .foreground,
      for: virtualMachine
    )

    XCTAssertTrue(didReapply)
    XCTAssertEqual(model.runtimeResourcePolicy(for: virtualMachine), policy)
    XCTAssertEqual(
      model.alertMessage,
      "Runtime resource policy recorded for dev; display helper acknowledged it; live apply blocked: runtime-control-unavailable: Live Apple VZ CPU/RAM hot-apply is not implemented yet; the policy is available to display pacing and runtime policy IPC consumers."
    )
  }

  func testCreateDiagnosticBundleStoresResult() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let bundle = DiagnosticBundle(
      vm: "Dev VM",
      source: "/tmp/Dev.vmbridge",
      output: "/tmp/diagnostics/Dev",
      files: ["manifest.yaml", "logs/qemu.log"],
      createdAtUnix: 1_710_000_400
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      diagnosticBundleResult: .success(bundle)
    )
    let model = DashboardViewModel(client: client)

    let didCreate = await model.createDiagnosticBundle(
      output: "  /tmp/diagnostics  ",
      for: virtualMachine
    )

    XCTAssertTrue(didCreate)
    XCTAssertEqual(model.diagnosticBundle(for: virtualMachine), bundle)
    XCTAssertNil(model.diagnosticBundleError(for: virtualMachine))
    XCTAssertEqual(client.createdDiagnosticBundleRequests.count, 1)
    XCTAssertEqual(client.createdDiagnosticBundleRequests[0].output, "/tmp/diagnostics")
    XCTAssertEqual(client.createdDiagnosticBundleRequests[0].id, virtualMachine.id)
    XCTAssertEqual(model.alertMessage, "Diagnostic bundle created at /tmp/diagnostics/Dev.")
    XCTAssertNil(model.creatingDiagnosticBundleID)
  }

  func testUpdateClientIgnoresStaleDiagnosticBundleResult() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let bundle = DiagnosticBundle(
      vm: "Dev VM",
      source: "/tmp/Dev.vmbridge",
      output: "/tmp/diagnostics/Dev",
      files: ["manifest.yaml", "logs/qemu.log"],
      createdAtUnix: 1_710_000_400
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      diagnosticBundleResult: .success(bundle),
      diagnosticBundleDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    let staleBundle = Task {
      await model.createDiagnosticBundle(output: "/tmp/diagnostics", for: virtualMachine)
    }
    for _ in 0..<100 where model.creatingDiagnosticBundleID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    let didCreate = await staleBundle.value

    XCTAssertFalse(didCreate)
    XCTAssertNil(model.diagnosticBundle(for: virtualMachine))
    XCTAssertNil(model.diagnosticBundleError(for: virtualMachine))
    XCTAssertNil(model.creatingDiagnosticBundleID)
    XCTAssertNil(model.alertMessage)
    XCTAssertEqual(oldClient.createdDiagnosticBundleRequests.count, 1)
  }

  func testCreatePerformanceBaselineStoresMetadataOnlyResult() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let baseline = PerformanceBaseline(
      vm: "Dev VM",
      source: "/tmp/Dev.vmbridge",
      output: "/tmp/perf",
      artifact: "/tmp/perf/performance-baseline.json",
      createdAtUnix: 1_710_000_500,
      metadataOnly: true,
      state: .running,
      runner: nil,
      guestTools: DashboardViewModelTests.guestToolsStatus(vm: "Dev VM"),
      metrics: nil,
      measurements: [
        PerformanceMeasurement(
          name: "guest_tools_connected",
          value: 1,
          unit: "bool",
          source: "metadata.guest_tools",
          metadataOnly: true
        )
      ],
      notes: ["metadata-only baseline; no benchmark workloads were executed"]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      performanceBaselineResult: .success(baseline)
    )
    let model = DashboardViewModel(client: client)

    let didCreate = await model.createPerformanceBaseline(
      output: " /tmp/perf ", for: virtualMachine)

    XCTAssertTrue(didCreate)
    XCTAssertEqual(model.performanceBaseline(for: virtualMachine), baseline)
    XCTAssertNil(model.performanceBaselineError(for: virtualMachine))
    XCTAssertEqual(client.createdPerformanceBaselineRequests.count, 1)
    XCTAssertEqual(client.createdPerformanceBaselineRequests[0].output, "/tmp/perf")
    XCTAssertEqual(client.createdPerformanceBaselineRequests[0].id, virtualMachine.id)
    XCTAssertEqual(
      model.alertMessage,
      "Performance baseline metadata created at /tmp/perf/performance-baseline.json."
    )
    XCTAssertNil(model.creatingPerformanceBaselineID)
  }

  func testUpdateClientIgnoresStalePerformanceBaselineResult() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let baseline = PerformanceBaseline(
      vm: "Dev VM",
      source: "/tmp/old/Dev.vmbridge",
      output: "/tmp/old/perf",
      artifact: "/tmp/old/perf/performance-baseline.json",
      createdAtUnix: 1_710_000_500,
      metadataOnly: true,
      state: .running,
      runner: nil,
      guestTools: DashboardViewModelTests.guestToolsStatus(vm: "Dev VM"),
      metrics: nil,
      measurements: [],
      notes: ["metadata-only baseline; no benchmark workloads were executed"]
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      performanceBaselineResult: .success(baseline),
      performanceBaselineDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    let staleBaseline = Task {
      await model.createPerformanceBaseline(output: "/tmp/old/perf", for: virtualMachine)
    }
    for _ in 0..<100 where model.creatingPerformanceBaselineID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    let didCreate = await staleBaseline.value

    XCTAssertFalse(didCreate)
    XCTAssertNil(model.performanceBaseline(for: virtualMachine))
    XCTAssertNil(model.performanceBaselineError(for: virtualMachine))
    XCTAssertNil(model.creatingPerformanceBaselineID)
    XCTAssertNil(model.alertMessage)
    XCTAssertEqual(oldClient.createdPerformanceBaselineRequests.count, 1)
  }

  func testCreatePerformanceSampleStoresBoundedHostSideResult() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let sample = PerformanceSample(
      vm: "Dev VM",
      source: "/tmp/Dev.vmbridge",
      output: "/tmp/perf",
      artifact: "/tmp/perf/performance-sample.json",
      probe: "/tmp/perf/probe-1.bin",
      probes: ["/tmp/perf/probe-1.bin"],
      artifactBytes: 4096,
      iterations: 1,
      sync: false,
      iterationResults: [
        PerformanceSampleIteration(
          iteration: 1,
          probe: "/tmp/perf/probe-1.bin",
          bytes: 4096,
          writeLatencyMicroseconds: 120,
          sync: false
        )
      ],
      createdAtUnix: 1_710_000_600,
      state: .running,
      runner: nil,
      guestTools: DashboardViewModelTests.guestToolsStatus(vm: "Dev VM"),
      metrics: nil,
      measurements: [
        PerformanceMeasurement(
          name: "host_artifact_write_total_bytes",
          value: 4096,
          unit: "bytes",
          source: "host.fs.write_probe",
          metadataOnly: false
        )
      ],
      notes: ["host-side sample; no guest benchmark workloads were executed"]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      performanceSampleResult: .success(sample)
    )
    let model = DashboardViewModel(client: client)

    let didCreate = await model.createPerformanceSample(
      output: " /tmp/perf ",
      artifactBytes: " 4096 ",
      iterations: " 1 ",
      sync: false,
      for: virtualMachine
    )

    XCTAssertTrue(didCreate)
    XCTAssertEqual(model.performanceSample(for: virtualMachine), sample)
    XCTAssertNil(model.performanceSampleError(for: virtualMachine))
    XCTAssertEqual(client.createdPerformanceSampleRequests.count, 1)
    XCTAssertEqual(client.createdPerformanceSampleRequests[0].output, "/tmp/perf")
    XCTAssertEqual(client.createdPerformanceSampleRequests[0].artifactBytes, 4096)
    XCTAssertEqual(client.createdPerformanceSampleRequests[0].iterations, 1)
    XCTAssertFalse(client.createdPerformanceSampleRequests[0].sync)
    XCTAssertEqual(client.createdPerformanceSampleRequests[0].id, virtualMachine.id)
    XCTAssertEqual(
      model.alertMessage,
      "Performance sample metadata created at /tmp/perf/performance-sample.json."
    )
    XCTAssertNil(model.creatingPerformanceSampleID)
  }

  func testUpdateClientIgnoresStalePerformanceSampleResult() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let sample = PerformanceSample(
      vm: "Dev VM",
      source: "/tmp/old/Dev.vmbridge",
      output: "/tmp/old/perf",
      artifact: "/tmp/old/perf/performance-sample.json",
      probe: "/tmp/old/perf/probe-1.bin",
      probes: ["/tmp/old/perf/probe-1.bin"],
      artifactBytes: 4096,
      iterations: 1,
      sync: false,
      iterationResults: [
        PerformanceSampleIteration(
          iteration: 1,
          probe: "/tmp/old/perf/probe-1.bin",
          bytes: 4096,
          writeLatencyMicroseconds: 120,
          sync: false
        )
      ],
      createdAtUnix: 1_710_000_600,
      state: .running,
      runner: nil,
      guestTools: DashboardViewModelTests.guestToolsStatus(vm: "Dev VM"),
      metrics: nil,
      measurements: [],
      notes: ["host-side sample; no guest benchmark workloads were executed"]
    )
    let oldClient = StubVirtualMachineClient(
      sourceTitle: "Old inventory",
      listResult: .success([virtualMachine]),
      performanceSampleResult: .success(sample),
      performanceSampleDelayNanos: 80_000_000
    )
    let newClient = StubVirtualMachineClient(
      sourceTitle: "New inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: oldClient)

    let staleSample = Task {
      await model.createPerformanceSample(
        output: "/tmp/old/perf",
        artifactBytes: "4096",
        iterations: "1",
        sync: false,
        for: virtualMachine
      )
    }
    for _ in 0..<100 where model.creatingPerformanceSampleID == nil {
      try await Task.sleep(nanoseconds: 1_000_000)
    }

    model.updateClient(newClient)
    let didCreate = await staleSample.value

    XCTAssertFalse(didCreate)
    XCTAssertNil(model.performanceSample(for: virtualMachine))
    XCTAssertNil(model.performanceSampleError(for: virtualMachine))
    XCTAssertNil(model.creatingPerformanceSampleID)
    XCTAssertNil(model.alertMessage)
    XCTAssertEqual(oldClient.createdPerformanceSampleRequests.count, 1)
  }

  func testDiagnosticAndPerformanceArtifactsRequireOutputBeforeClientRequest() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: client)

    let didCreateBundle = await model.createDiagnosticBundle(output: " ", for: virtualMachine)
    XCTAssertFalse(didCreateBundle)
    XCTAssertTrue(client.createdDiagnosticBundleRequests.isEmpty)
    XCTAssertEqual(
      model.diagnosticBundleError(for: virtualMachine),
      "Choose an output folder for this metadata artifact."
    )

    let didCreateBaseline = await model.createPerformanceBaseline(output: "", for: virtualMachine)
    XCTAssertFalse(didCreateBaseline)
    XCTAssertTrue(client.createdPerformanceBaselineRequests.isEmpty)
    XCTAssertEqual(
      model.performanceBaselineError(for: virtualMachine),
      "Choose an output folder for this metadata artifact."
    )

    let didCreateSample = await model.createPerformanceSample(
      output: "   ",
      artifactBytes: "4096",
      iterations: "1",
      sync: false,
      for: virtualMachine
    )
    XCTAssertFalse(didCreateSample)
    XCTAssertTrue(client.createdPerformanceSampleRequests.isEmpty)
    XCTAssertEqual(
      model.performanceSampleError(for: virtualMachine),
      "Choose an output folder for this metadata artifact."
    )
    XCTAssertEqual(model.alertMessage, "Choose an output folder for this metadata artifact.")
  }

  func testCreatePerformanceSampleRejectsOversizedProbeBeforeClientRequest() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: client)

    let didCreate = await model.createPerformanceSample(
      output: "/tmp/perf",
      artifactBytes: "67108865",
      iterations: "1",
      sync: false,
      for: virtualMachine
    )

    XCTAssertFalse(didCreate)
    XCTAssertTrue(client.createdPerformanceSampleRequests.isEmpty)
    XCTAssertEqual(model.alertMessage, "Performance probe byte count must be 64 MiB or smaller.")
    XCTAssertNil(model.creatingPerformanceSampleID)
  }

  func testImportBootMediaRefreshesStatusAfterSuccessfulImport() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let status = BootMediaStatus(
      vm: "Dev VM",
      entries: [
        BootMediaStatusEntry(
          kind: .installerImage,
          path: "installers/dev.iso",
          exists: true,
          sizeBytes: 28,
          lastImport: BootMediaImportMetadata(
            vm: "Dev VM",
            kind: .installerImage,
            source: "/tmp/dev.iso",
            destination: "installers/dev.iso",
            bytes: 28,
            replaced: false,
            importedAtUnix: 1_710_000_040
          ),
          lastVerification: nil,
          lastDownloadPlan: nil,
          lastDownload: nil
        )
      ]
    )
    let imported = BootMediaImportMetadata(
      vm: "Dev VM",
      kind: .installerImage,
      source: "/tmp/dev.iso",
      destination: "installers/dev.iso",
      bytes: 28,
      replaced: false,
      importedAtUnix: 1_710_000_040
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      bootMediaStatusResult: .success(status),
      importResult: .success(imported)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didImport = await model.importBootMedia(
      sourcePath: "  /tmp/dev.iso  ",
      kind: .installerImage,
      for: virtualMachine
    )

    XCTAssertTrue(didImport)
    XCTAssertEqual(client.importedBootMediaRequests.count, 1)
    XCTAssertEqual(client.importedBootMediaRequests[0].sourcePath, "/tmp/dev.iso")
    XCTAssertEqual(client.importedBootMediaRequests[0].kind, .installerImage)
    XCTAssertEqual(client.importedBootMediaRequests[0].id, virtualMachine.id)
    XCTAssertEqual(model.bootMediaStatus(for: virtualMachine), status)
    XCTAssertEqual(client.inspectedBootMediaStatusIDs, [virtualMachine.id])
    XCTAssertEqual(model.alertMessage, "Installer image imported from /tmp/dev.iso.")
  }

  func testVerifyBootMediaRefreshesStatusAfterSuccessfulVerification() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let verification = BootMediaVerificationMetadata(
      vm: "Dev VM",
      kind: .installerImage,
      path: "installers/dev.iso",
      bytes: 28,
      expectedSHA256: "abc",
      actualSHA256: "abc",
      verified: true,
      verifiedAtUnix: 1_710_000_050
    )
    let status = BootMediaStatus(
      vm: "Dev VM",
      entries: [
        BootMediaStatusEntry(
          kind: .installerImage,
          path: "installers/dev.iso",
          exists: true,
          sizeBytes: 28,
          lastImport: nil,
          lastVerification: verification,
          lastDownloadPlan: nil,
          lastDownload: nil
        )
      ]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      bootMediaStatusResult: .success(status),
      verificationResult: .success(verification)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didVerify = await model.verifyBootMedia(
      expectedSHA256: "  abc  ",
      kind: .installerImage,
      for: virtualMachine
    )

    XCTAssertTrue(didVerify)
    XCTAssertEqual(client.verifiedBootMediaRequests.count, 1)
    XCTAssertEqual(client.verifiedBootMediaRequests[0].expectedSHA256, "abc")
    XCTAssertEqual(client.verifiedBootMediaRequests[0].kind, .installerImage)
    XCTAssertEqual(client.verifiedBootMediaRequests[0].id, virtualMachine.id)
    XCTAssertEqual(model.bootMediaStatus(for: virtualMachine), status)
    XCTAssertEqual(client.inspectedBootMediaStatusIDs, [virtualMachine.id])
    XCTAssertEqual(model.alertMessage, "Installer image verified.")
  }

  func testPlanBootMediaDownloadRefreshesStatusAfterSuccessfulPlan() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let plan = BootMediaDownloadPlanMetadata(
      vm: "Dev VM",
      kind: .installerImage,
      url: "https://example.invalid/dev.iso",
      destination: "installers/dev.iso",
      exists: false,
      bytes: nil,
      expectedSHA256: nil,
      plannedAtUnix: 1_710_000_060
    )
    let status = BootMediaStatus(
      vm: "Dev VM",
      entries: [
        BootMediaStatusEntry(
          kind: .installerImage,
          path: "installers/dev.iso",
          exists: false,
          sizeBytes: nil,
          lastImport: nil,
          lastVerification: nil,
          lastDownloadPlan: plan,
          lastDownload: nil
        )
      ]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      bootMediaStatusResult: .success(status),
      downloadPlanResult: .success(plan)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didPlan = await model.planBootMediaDownload(
      url: "  https://example.invalid/dev.iso  ",
      expectedSHA256: "   ",
      kind: .installerImage,
      for: virtualMachine
    )

    XCTAssertTrue(didPlan)
    XCTAssertEqual(client.plannedBootMediaDownloadRequests.count, 1)
    XCTAssertEqual(
      client.plannedBootMediaDownloadRequests[0].url, "https://example.invalid/dev.iso")
    XCTAssertNil(client.plannedBootMediaDownloadRequests[0].expectedSHA256)
    XCTAssertEqual(client.plannedBootMediaDownloadRequests[0].kind, .installerImage)
    XCTAssertEqual(client.plannedBootMediaDownloadRequests[0].id, virtualMachine.id)
    XCTAssertEqual(model.bootMediaStatus(for: virtualMachine), status)
    XCTAssertEqual(client.inspectedBootMediaStatusIDs, [virtualMachine.id])
    XCTAssertEqual(model.alertMessage, "Installer image download planned for installers/dev.iso.")
  }

  func testDownloadBootMediaRefreshesStatusAfterSuccessfulDownload() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let download = BootMediaDownloadResultMetadata(
      vm: "Dev VM",
      kind: .installerImage,
      url: "https://example.invalid/dev.iso",
      destination: "installers/dev.iso",
      bytes: 28,
      replaced: false,
      expectedSHA256: "abc",
      actualSHA256: "abc",
      verified: true,
      downloaded: true,
      downloadedAtUnix: 1_710_000_070
    )
    let status = BootMediaStatus(
      vm: "Dev VM",
      entries: [
        BootMediaStatusEntry(
          kind: .installerImage,
          path: "installers/dev.iso",
          exists: true,
          sizeBytes: 28,
          lastImport: nil,
          lastVerification: nil,
          lastDownloadPlan: nil,
          lastDownload: download
        )
      ]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      bootMediaStatusResult: .success(status),
      downloadResult: .success(download)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didDownload = await model.downloadBootMedia(
      kind: .installerImage,
      for: virtualMachine
    )

    XCTAssertTrue(didDownload)
    XCTAssertEqual(client.downloadedBootMediaRequests.count, 1)
    XCTAssertEqual(client.downloadedBootMediaRequests[0].kind, .installerImage)
    XCTAssertEqual(client.downloadedBootMediaRequests[0].id, virtualMachine.id)
    XCTAssertEqual(model.bootMediaStatus(for: virtualMachine), status)
    XCTAssertEqual(client.inspectedBootMediaStatusIDs, [virtualMachine.id])
    XCTAssertEqual(model.alertMessage, "Installer image downloaded to installers/dev.iso.")
  }

  func testOpenConsoleReportsReadyQMPStatus() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let qmp = QMPStatus(
      socketPath: "/tmp/dev-qmp.sock",
      available: true,
      status: "running",
      running: true
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      qmpStatusResult: .success(qmp)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didOpen = await model.openConsole(for: virtualMachine)

    XCTAssertTrue(didOpen)
    XCTAssertNil(model.openingConsoleID)
    XCTAssertEqual(client.inspectedQMPStatusIDs, [virtualMachine.id])
    XCTAssertEqual(model.qmpStatus(for: virtualMachine), qmp)
    XCTAssertNil(model.qmpStatusError(for: virtualMachine))
    XCTAssertEqual(
      model.alertMessage,
      "QMP diagnostics socket available at /tmp/dev-qmp.sock (running)."
    )
  }

  func testOpenConsoleFetchesQemuLaunchPlanAndOpensVNCViewer() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu x86_64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let plan = QemuLaunchPlan(
      program: "qemu-system-x86_64",
      args: ["-name", "Dev VM", "-display", "vnc=:0"]
    )
    let qmp = QMPStatus(
      socketPath: "/tmp/dev-qmp.sock",
      available: true,
      status: "running",
      running: true
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      qmpStatusResult: .success(qmp),
      qemuLaunchPlanResult: .success(plan)
    )
    var openedURLs: [URL] = []
    let model = DashboardViewModel(
      client: client,
      openExternalURL: { url in
        openedURLs.append(url)
        return true
      }
    )

    await model.load()
    let didOpen = await model.openConsole(for: virtualMachine)

    XCTAssertTrue(didOpen)
    XCTAssertEqual(client.inspectedQemuLaunchPlanIDs, [virtualMachine.id])
    XCTAssertTrue(client.inspectedQMPStatusIDs.isEmpty)
    XCTAssertEqual(model.qemuLaunchPlan(for: virtualMachine), plan)
    XCTAssertEqual(openedURLs.map(\.absoluteString), ["vnc://127.0.0.1:5900"])
    XCTAssertEqual(model.alertMessage, "Opened VNC viewer at vnc://127.0.0.1:5900.")
  }

  func testOpenConsoleIgnoresStaleCachedVNCPlanAfterVMModeChanges() async throws {
    let vmID = UUID()
    let compatibilityVM = VirtualMachine(
      id: vmID,
      name: "Dev VM",
      guest: "Ubuntu x86_64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let fastVM = VirtualMachine(
      id: vmID,
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "13m",
      ipAddress: "192.168.64.24",
      lastStarted: nil,
      notes: "test"
    )
    let plan = QemuLaunchPlan(
      program: "qemu-system-x86_64",
      args: ["-name", "Dev VM", "-display", "vnc=:0"]
    )
    let qmp = QMPStatus(
      socketPath: "/tmp/dev-qmp.sock",
      available: true,
      status: "running",
      running: true
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([compatibilityVM]),
      qmpStatusResult: .success(qmp),
      qemuLaunchPlanResult: .success(plan)
    )
    var openedURLs: [URL] = []
    let model = DashboardViewModel(
      client: client,
      openExternalURL: { url in
        openedURLs.append(url)
        return true
      }
    )

    await model.load()
    let didOpenCompatibilityConsole = await model.openConsole(for: compatibilityVM)
    XCTAssertTrue(didOpenCompatibilityConsole)
    XCTAssertEqual(openedURLs.map(\.absoluteString), ["vnc://127.0.0.1:5900"])
    XCTAssertEqual(model.qemuLaunchPlan(for: compatibilityVM), plan)

    client.listResult = .success([fastVM])
    await model.load()
    XCTAssertNil(model.qemuLaunchPlan(for: fastVM))
    let didOpenFastConsole = await model.openConsole(for: fastVM)
    XCTAssertTrue(didOpenFastConsole)

    XCTAssertEqual(client.inspectedQemuLaunchPlanIDs, [vmID])
    XCTAssertEqual(client.inspectedQMPStatusIDs, [vmID])
    XCTAssertEqual(openedURLs.map(\.absoluteString), ["vnc://127.0.0.1:5900"])
    XCTAssertEqual(
      model.alertMessage,
      "QMP diagnostics socket available at /tmp/dev-qmp.sock (running)."
    )
  }

  func testOpenConsoleIgnoresConcurrentVNCViewerHandoffForSameVM() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu x86_64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let plan = QemuLaunchPlan(
      program: "qemu-system-x86_64",
      args: ["-name", "Dev VM", "-display", "vnc=:0"]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      qemuLaunchPlanResult: .success(plan),
      qemuLaunchPlanDelayNanos: 50_000_000
    )
    var openedURLs: [URL] = []
    let model = DashboardViewModel(
      client: client,
      openExternalURL: { url in
        openedURLs.append(url)
        return true
      }
    )

    await model.load()
    async let firstOpen = model.openConsole(for: virtualMachine)
    async let secondOpen = model.openConsole(for: virtualMachine)
    let results = await [firstOpen, secondOpen]

    XCTAssertEqual(results.filter { $0 }.count, 1)
    XCTAssertEqual(client.inspectedQemuLaunchPlanIDs, [virtualMachine.id])
    XCTAssertEqual(openedURLs.map(\.absoluteString), ["vnc://127.0.0.1:5900"])
    XCTAssertNil(model.openingConsoleID)
    XCTAssertEqual(model.alertMessage, "Opened VNC viewer at vnc://127.0.0.1:5900.")
  }

  func testOpenConsoleReportsVNCViewerOpenFailureAndClearsItAfterSuccessfulRetry()
    async throws
  {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu x86_64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let plan = QemuLaunchPlan(
      program: "qemu-system-x86_64",
      args: ["-name", "Dev VM", "-display", "vnc=:0"]
    )
    let qmp = QMPStatus(
      socketPath: "/tmp/dev-qmp.sock",
      available: true,
      status: "running",
      running: true
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      qmpStatusResult: .success(qmp),
      qemuLaunchPlanResult: .success(plan)
    )
    var shouldOpenViewer = false
    var openedURLs: [URL] = []
    let model = DashboardViewModel(
      client: client,
      openExternalURL: { url in
        openedURLs.append(url)
        return shouldOpenViewer
      }
    )

    await model.load()
    let didOpenFirstAttempt = await model.openConsole(for: virtualMachine)

    XCTAssertTrue(didOpenFirstAttempt)
    XCTAssertEqual(client.inspectedQemuLaunchPlanIDs, [virtualMachine.id])
    XCTAssertEqual(client.inspectedQMPStatusIDs, [virtualMachine.id])
    XCTAssertEqual(openedURLs.map(\.absoluteString), ["vnc://127.0.0.1:5900"])
    XCTAssertEqual(
      model.qemuLaunchPlanError(for: virtualMachine),
      "macOS could not open vnc://127.0.0.1:5900. Open it manually with your VNC viewer."
    )
    XCTAssertEqual(
      model.alertMessage,
      "VNC viewer handoff failed: macOS could not open vnc://127.0.0.1:5900. Open it manually with your VNC viewer. QMP diagnostics socket available at /tmp/dev-qmp.sock (running)."
    )

    shouldOpenViewer = true
    let didOpenSecondAttempt = await model.openConsole(for: virtualMachine)

    XCTAssertTrue(didOpenSecondAttempt)
    XCTAssertEqual(client.inspectedQemuLaunchPlanIDs, [virtualMachine.id])
    XCTAssertEqual(client.inspectedQMPStatusIDs, [virtualMachine.id])
    XCTAssertEqual(
      openedURLs.map(\.absoluteString),
      ["vnc://127.0.0.1:5900", "vnc://127.0.0.1:5900"]
    )
    XCTAssertNil(model.qemuLaunchPlanError(for: virtualMachine))
    XCTAssertEqual(model.alertMessage, "Opened VNC viewer at vnc://127.0.0.1:5900.")
  }

  func testOpenConsoleFallsBackToQMPDiagnosticsWhenQemuLaunchPlanFetchFails() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu x86_64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let qmp = QMPStatus(
      socketPath: "/tmp/dev-qmp.sock",
      available: true,
      status: "running",
      running: true
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      qmpStatusResult: .success(qmp),
      qemuLaunchPlanResult: .failure(TestRefreshError.offline)
    )
    var openedURLs: [URL] = []
    let model = DashboardViewModel(
      client: client,
      openExternalURL: { url in
        openedURLs.append(url)
        return true
      }
    )

    await model.load()
    let didOpen = await model.openConsole(for: virtualMachine)

    XCTAssertTrue(didOpen)
    XCTAssertEqual(client.inspectedQemuLaunchPlanIDs, [virtualMachine.id])
    XCTAssertEqual(client.inspectedQMPStatusIDs, [virtualMachine.id])
    XCTAssertTrue(openedURLs.isEmpty)
    XCTAssertEqual(model.qemuLaunchPlanError(for: virtualMachine), "Offline")
    XCTAssertEqual(
      model.alertMessage,
      "VNC viewer endpoint unavailable: Offline. QMP diagnostics socket available at /tmp/dev-qmp.sock (running)."
    )
  }

  func testOpenConsoleReportsViewerFailureAndQMPFallbackFailure() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu x86_64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let plan = QemuLaunchPlan(
      program: "qemu-system-x86_64",
      args: ["-name", "Dev VM", "-display", "vnc=:0"]
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      qmpStatusResult: .failure(VirtualMachineClientError.virtualMachineNotFound),
      qemuLaunchPlanResult: .success(plan)
    )
    var openedURLs: [URL] = []
    let model = DashboardViewModel(
      client: client,
      openExternalURL: { url in
        openedURLs.append(url)
        return false
      }
    )

    await model.load()
    let didOpen = await model.openConsole(for: virtualMachine)

    XCTAssertFalse(didOpen)
    XCTAssertEqual(client.inspectedQemuLaunchPlanIDs, [virtualMachine.id])
    XCTAssertEqual(client.inspectedQMPStatusIDs, [virtualMachine.id])
    XCTAssertEqual(openedURLs.map(\.absoluteString), ["vnc://127.0.0.1:5900"])
    XCTAssertEqual(
      model.qemuLaunchPlanError(for: virtualMachine),
      "macOS could not open vnc://127.0.0.1:5900. Open it manually with your VNC viewer."
    )
    XCTAssertEqual(
      model.qmpStatusError(for: virtualMachine),
      VirtualMachineClientError.virtualMachineNotFound.localizedDescription
    )
    XCTAssertEqual(
      model.alertMessage,
      "VNC viewer handoff failed for Dev VM: macOS could not open vnc://127.0.0.1:5900. Open it manually with your VNC viewer. QMP diagnostics also failed: The selected virtual machine could not be found."
    )
  }

  func testOpenConsoleReportsViewerFailureAndUnavailableQMPFallback() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu x86_64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let plan = QemuLaunchPlan(
      program: "qemu-system-x86_64",
      args: ["-name", "Dev VM", "-display", "vnc=:0"]
    )
    let qmp = QMPStatus(
      socketPath: "/tmp/missing-qmp.sock",
      available: false,
      status: nil,
      running: nil
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      qmpStatusResult: .success(qmp),
      qemuLaunchPlanResult: .success(plan)
    )
    let model = DashboardViewModel(
      client: client,
      openExternalURL: { _ in false }
    )

    await model.load()
    let didOpen = await model.openConsole(for: virtualMachine)

    XCTAssertFalse(didOpen)
    XCTAssertEqual(client.inspectedQemuLaunchPlanIDs, [virtualMachine.id])
    XCTAssertEqual(client.inspectedQMPStatusIDs, [virtualMachine.id])
    XCTAssertEqual(model.qmpStatus(for: virtualMachine), qmp)
    XCTAssertEqual(
      model.alertMessage,
      "VNC viewer handoff failed: macOS could not open vnc://127.0.0.1:5900. Open it manually with your VNC viewer. Console diagnostics are not available yet. Expected QMP socket: /tmp/missing-qmp.sock"
    )
  }

  func testLoadLogViewStoresSelectedVmLogTail() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let log = VMLogView(
      vm: "Dev VM",
      kind: .qemu,
      path: "/tmp/dev.vmbridge/logs/qemu.log",
      exists: true,
      bytes: 128,
      returnedBytes: 32,
      truncated: true,
      content: "qemu tail"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      logViewResult: .success(log)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    await model.loadLogView(kind: .qemu, for: virtualMachine)

    XCTAssertEqual(model.logView(kind: .qemu, for: virtualMachine), log)
    XCTAssertNil(model.logViewError(for: virtualMachine))
    XCTAssertNil(model.loadingLogViewID)
    XCTAssertEqual(client.viewedLogRequests.count, 1)
    XCTAssertEqual(client.viewedLogRequests[0].kind, .qemu)
    XCTAssertEqual(client.viewedLogRequests[0].bytes, 16 * 1024)
    XCTAssertEqual(client.viewedLogRequests[0].id, virtualMachine.id)
  }

  func testOpenConsoleReportsUnavailableQMPStatus() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .paused,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let qmp = QMPStatus(
      socketPath: "/tmp/missing-qmp.sock",
      available: false,
      status: nil,
      running: nil
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      qmpStatusResult: .success(qmp)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didOpen = await model.openConsole(for: virtualMachine)

    XCTAssertFalse(didOpen)
    XCTAssertNil(model.openingConsoleID)
    XCTAssertEqual(client.inspectedQMPStatusIDs, [virtualMachine.id])
    XCTAssertEqual(model.qmpStatus(for: virtualMachine), qmp)
    XCTAssertNil(model.qmpStatusError(for: virtualMachine))
    XCTAssertEqual(
      model.alertMessage,
      "Console diagnostics are not available yet. Expected QMP socket: /tmp/missing-qmp.sock"
    )
  }

  func testOpenConsoleStoresQMPProbeErrorForDiagnostics() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      qmpStatusResult: .failure(VirtualMachineClientError.virtualMachineNotFound)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didOpen = await model.openConsole(for: virtualMachine)

    XCTAssertFalse(didOpen)
    XCTAssertNil(model.openingConsoleID)
    XCTAssertEqual(client.inspectedQMPStatusIDs, [virtualMachine.id])
    XCTAssertNil(model.qmpStatus(for: virtualMachine))
    XCTAssertEqual(
      model.qmpStatusError(for: virtualMachine),
      VirtualMachineClientError.virtualMachineNotFound.localizedDescription
    )
  }

  func testOpenConsoleClearsStaleQMPStatusWhenProbeFailsAfterSuccess() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.23",
      lastStarted: nil,
      notes: "test"
    )
    let qmp = QMPStatus(
      socketPath: "/tmp/dev-qmp.sock",
      available: true,
      status: "running",
      running: true
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      qmpStatusResult: .success(qmp)
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didOpenFirstAttempt = await model.openConsole(for: virtualMachine)
    XCTAssertTrue(didOpenFirstAttempt)
    XCTAssertEqual(model.qmpStatus(for: virtualMachine), qmp)

    client.qmpStatusResult = .failure(VirtualMachineClientError.virtualMachineNotFound)
    let didOpen = await model.openConsole(for: virtualMachine)

    XCTAssertFalse(didOpen)
    XCTAssertEqual(client.inspectedQMPStatusIDs, [virtualMachine.id, virtualMachine.id])
    XCTAssertNil(model.qmpStatus(for: virtualMachine))
    XCTAssertEqual(
      model.qmpStatusError(for: virtualMachine),
      VirtualMachineClientError.virtualMachineNotFound.localizedDescription
    )
  }

  func testOpenConsoleRejectsStoppedVirtualMachineWithoutQMPRequest() async throws {
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(client: client)

    await model.load()
    let didOpen = await model.openConsole(for: virtualMachine)

    XCTAssertFalse(didOpen)
    XCTAssertTrue(client.inspectedQMPStatusIDs.isEmpty)
    XCTAssertEqual(
      model.alertMessage,
      "Console diagnostics and VNC viewer handoff are available for running or paused virtual machines."
    )
  }

  func testOpenGuestWindowProxyBuildsPlanAndOpensShell() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    let window = GuestToolsWindowAction(
      id: "0x01200007",
      title: "Terminal",
      source: "wmctrl",
      focused: true,
      desktop: 0,
      pid: 4242,
      bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600)
    )
    let status = GuestToolsStatus(
      vm: "Dev VM",
      tools: "required",
      tokenCreatedAtUnix: 1_710_000_000,
      capabilities: [
        GuestToolsCapability(name: "windows", maxVersion: 1, enabledBy: "integration.windows")
      ],
      runtime: GuestToolsRuntime(
        connected: true,
        guestOS: "ubuntu",
        agentVersion: "0.1.0",
        capabilities: ["windows"],
        lastHeartbeatAtUnix: 1_710_000_060,
        guestIPAddresses: [],
        sharedFolders: [],
        metrics: nil,
        updatedAtUnix: 1_710_000_062
      )
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(status),
      guestToolsCommandDispatchResult: .success(
        GuestToolsCommandDispatch(vm: "Dev VM", requestID: "window-input", pendingCommands: 1)
      )
    )
    var openedPlan: GuestWindowProxyPlan?
    var openedInputSender: GuestWindowProxyInputSender?
    let model = DashboardViewModel(
      client: client,
      openGuestWindowProxyShell: { plan, inputSender in
        openedPlan = plan
        openedInputSender = inputSender
      }
    )

    XCTAssertTrue(model.openGuestWindowProxy(for: window, in: virtualMachine))

    let plan = try XCTUnwrap(openedPlan)
    XCTAssertEqual(plan.vmName, "Dev VM")
    XCTAssertEqual(plan.windowID, "0x01200007")
    XCTAssertEqual(plan.title, "Terminal")
    XCTAssertEqual(plan.guestBounds, GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600))
    XCTAssertEqual(plan.hostSize, GuestWindowProxyPlan.HostSize(width: 800, height: 600))
    XCTAssertEqual(plan.scale, 1.0, accuracy: 0.0001)
    XCTAssertEqual(plan.inputScaleX, 1.0, accuracy: 0.0001)
    XCTAssertEqual(plan.inputScaleY, 1.0, accuracy: 0.0001)
    XCTAssertEqual(plan.pid, 4242)
    XCTAssertEqual(plan.desktop, 0)
    XCTAssertNotNil(openedInputSender)

    openedInputSender?(
      .pointer(
        windowID: "0x01200007",
        point: GuestWindowProxyPlan.GuestPoint(x: 120, y: 240),
        action: .press,
        button: .left
      )
    )
    openedInputSender?(
      .pointer(
        windowID: "0x01200007",
        point: GuestWindowProxyPlan.GuestPoint(x: 120, y: 240),
        action: .release,
        button: .left
      )
    )
    openedInputSender?(
      .bounds(
        windowID: "0x01200007",
        bounds: GuestToolsWindowBounds(x: 50, y: 60, width: 1024, height: 768)
      )
    )
    openedInputSender?(.close(windowID: "0x01200007"))
    for _ in 0..<50 {
      if client.sentGuestToolsCommandRequests.count == 4 {
        break
      }
      try await Task.sleep(nanoseconds: 10_000_000)
    }
    XCTAssertEqual(client.sentGuestToolsCommandRequests.count, 4)
    XCTAssertEqual(
      client.sentGuestToolsCommandRequests[0].command,
      .windowPointerInput(id: "0x01200007", x: 120, y: 240, action: .press, button: .left)
    )
    XCTAssertEqual(
      client.sentGuestToolsCommandRequests[1].command,
      .windowPointerInput(id: "0x01200007", x: 120, y: 240, action: .release, button: .left)
    )
    XCTAssertEqual(
      client.sentGuestToolsCommandRequests[2].command,
      .setWindowBounds(id: "0x01200007", x: 50, y: 60, width: 1024, height: 768)
    )
    XCTAssertEqual(
      client.sentGuestToolsCommandRequests[3].command,
      .closeWindow(id: "0x01200007")
    )
    XCTAssertTrue(client.sentGuestToolsCommandRequests[3].requestID?.hasPrefix("window-close-") == true)
    XCTAssertEqual(
      model.alertMessage,
      "Opened proxy shell for Terminal (0x01200007, pid 4242, guest 800x600 at 30,40, host 800x600)."
    )
  }

  func testCloseGuestWindowProxiesClosesTrackedShellsAndClearsStatus() {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    let terminalWindow = GuestToolsWindowAction(
      id: "0x01200007",
      title: "Terminal",
      source: "wmctrl",
      focused: true,
      desktop: 0,
      pid: 4242,
      bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600)
    )
    let filesWindow = GuestToolsWindowAction(
      id: "0x01200008",
      title: "Files",
      source: "wmctrl",
      focused: false,
      desktop: 0,
      pid: 4343,
      bounds: GuestToolsWindowBounds(x: 120, y: 160, width: 640, height: 480)
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine])
    )
    var closedProxyShells: [(vmName: String, windowID: String)] = []
    let model = DashboardViewModel(
      client: client,
      openGuestWindowProxyShell: { _, _ in },
      closeGuestWindowProxyShell: { vmName, windowID in
        closedProxyShells.append((vmName, windowID))
      }
    )

    XCTAssertTrue(model.openGuestWindowProxy(for: terminalWindow, in: virtualMachine))
    XCTAssertTrue(model.openGuestWindowProxy(for: filesWindow, in: virtualMachine))

    XCTAssertEqual(model.guestWindowProxyStatus(for: virtualMachine).trackedWindowCount, 2)
    XCTAssertEqual(model.closeGuestWindowProxies(for: virtualMachine), 2)

    XCTAssertEqual(closedProxyShells.map(\.vmName), ["Dev VM", "Dev VM"])
    XCTAssertEqual(Set(closedProxyShells.map(\.windowID)), ["0x01200007", "0x01200008"])
    XCTAssertEqual(model.guestWindowProxyStatus(for: virtualMachine), .idle)
    XCTAssertEqual(model.alertMessage, "Closed 2 proxy shells for Dev VM.")
  }

  func testGuestWindowProxyStatusSummarizesTrackedCropBackedWindows() {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    let terminalWindow = GuestToolsWindowAction(
      id: "0x01200007",
      title: "Terminal",
      source: "wmctrl",
      focused: true,
      desktop: 0,
      pid: 4242,
      bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600),
      cropFrameSummaryPath: "/tmp/terminal-crop.json"
    )
    let filesWindow = GuestToolsWindowAction(
      id: "0x01200008",
      title: "Files",
      source: "wmctrl",
      focused: false,
      desktop: 0,
      pid: 4343,
      bounds: GuestToolsWindowBounds(x: 120, y: 160, width: 640, height: 480)
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine])
    )
    let model = DashboardViewModel(
      client: client,
      openGuestWindowProxyShell: { _, _ in }
    )

    XCTAssertTrue(model.openGuestWindowProxy(for: filesWindow, in: virtualMachine))
    XCTAssertTrue(model.openGuestWindowProxy(for: terminalWindow, in: virtualMachine))

    let status = model.guestWindowProxyStatus(for: virtualMachine)
    XCTAssertEqual(status.trackedWindowCount, 2)
    XCTAssertEqual(status.cropBackedWindowCount, 1)
    XCTAssertEqual(
      status.trackedWindowSummaries,
      [
        "Terminal - 0x01200007 - 800x600 at 30,40 - crop",
        "Files - 0x01200008 - 640x480 at 120,160 - metadata",
      ]
    )
    XCTAssertEqual(status.detailText, "2 tracked windows, 1 crop-backed, auto refresh active")
    XCTAssertEqual(status.badgeTitle, "2 tracked, 1 crop")
    XCTAssertEqual(status.cropFrameText, "1/2 proxy shells have crop artifacts")
    XCTAssertEqual(
      status.windowSummaryText,
      "Terminal - 0x01200007 - 800x600 at 30,40 - crop | Files - 0x01200008 - 640x480 at 120,160 - metadata"
    )
  }

  func testOpenGuestWindowProxyRefreshesShellWhenWindowPayloadChanges() async throws {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    let window = GuestToolsWindowAction(
      id: "0x01200007",
      title: "Terminal",
      source: "wmctrl",
      focused: true,
      desktop: 0,
      pid: 4242,
      bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600),
      cropFrameSummaryPath: "/tmp/crop-a.json"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(
        guestToolsStatus(
          vm: "Dev VM",
          window: GuestToolsWindowAction(
            id: "0x01200007",
            title: "Terminal",
            source: "wmctrl",
            focused: true,
            desktop: 0,
            pid: 4242,
            bounds: GuestToolsWindowBounds(x: 50, y: 60, width: 1024, height: 768),
            cropFrameSummaryPath: "/tmp/crop-b.json"
          )
        )
      )
    )
    var openedPlans: [GuestWindowProxyPlan] = []
    let model = DashboardViewModel(
      client: client,
      openGuestWindowProxyShell: { plan, _ in
        openedPlans.append(plan)
      }
    )

    XCTAssertTrue(model.openGuestWindowProxy(for: window, in: virtualMachine))
    await model.loadGuestToolsStatus(for: virtualMachine)

    XCTAssertEqual(openedPlans.count, 2)
    XCTAssertEqual(
      openedPlans[0].guestBounds,
      GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600)
    )
    XCTAssertEqual(openedPlans[0].cropFrameSummaryPath, "/tmp/crop-a.json")
    XCTAssertEqual(
      openedPlans[1].guestBounds,
      GuestToolsWindowBounds(x: 50, y: 60, width: 1024, height: 768)
    )
    XCTAssertEqual(openedPlans[1].cropFrameSummaryPath, "/tmp/crop-b.json")
  }

  func testOpenGuestWindowProxyClosesOldShellWhenVmNameChangesDuringRefresh()
    async throws
  {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    var renamedVirtualMachine = virtualMachine
    renamedVirtualMachine.name = "Renamed VM"
    let window = GuestToolsWindowAction(
      id: "0x01200007",
      title: "Terminal",
      source: "wmctrl",
      focused: true,
      desktop: 0,
      pid: 4242,
      bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600)
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([renamedVirtualMachine]),
      guestToolsStatusResult: .success(
        guestToolsStatus(
          vm: "Renamed VM",
          window: GuestToolsWindowAction(
            id: "0x01200007",
            title: "Terminal",
            source: "wmctrl",
            focused: true,
            desktop: 0,
            pid: 4242,
            bounds: GuestToolsWindowBounds(x: 50, y: 60, width: 1024, height: 768)
          )
        )
      )
    )
    var openedPlans: [GuestWindowProxyPlan] = []
    var closedProxyShells: [(vmName: String, windowID: String)] = []
    let model = DashboardViewModel(
      client: client,
      openGuestWindowProxyShell: { plan, _ in
        openedPlans.append(plan)
      },
      closeGuestWindowProxyShell: { vmName, windowID in
        closedProxyShells.append((vmName, windowID))
      }
    )

    XCTAssertTrue(model.openGuestWindowProxy(for: window, in: virtualMachine))
    await model.loadGuestToolsStatus(for: renamedVirtualMachine)

    XCTAssertEqual(openedPlans.map(\.vmName), ["Dev VM", "Renamed VM"])
    XCTAssertEqual(closedProxyShells.count, 1)
    XCTAssertEqual(closedProxyShells[0].vmName, "Dev VM")
    XCTAssertEqual(closedProxyShells[0].windowID, "0x01200007")
  }

  func testOpenGuestWindowProxyReopensShellWhenInventoryRenamesVm()
    async throws
  {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    var renamedVirtualMachine = virtualMachine
    renamedVirtualMachine.name = "Renamed VM"
    let window = GuestToolsWindowAction(
      id: "0x01200007",
      title: "Terminal",
      source: "wmctrl",
      focused: true,
      desktop: 0,
      pid: 4242,
      bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600)
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([renamedVirtualMachine])
    )
    var openedPlans: [GuestWindowProxyPlan] = []
    var closedProxyShells: [(vmName: String, windowID: String)] = []
    let model = DashboardViewModel(
      client: client,
      openGuestWindowProxyShell: { plan, _ in
        openedPlans.append(plan)
      },
      closeGuestWindowProxyShell: { vmName, windowID in
        closedProxyShells.append((vmName, windowID))
      }
    )

    XCTAssertTrue(model.openGuestWindowProxy(for: window, in: virtualMachine))
    await model.load()

    XCTAssertEqual(openedPlans.map(\.vmName), ["Dev VM", "Renamed VM"])
    XCTAssertEqual(openedPlans.map(\.windowID), ["0x01200007", "0x01200007"])
    XCTAssertEqual(closedProxyShells.count, 1)
    XCTAssertEqual(closedProxyShells[0].vmName, "Dev VM")
    XCTAssertEqual(closedProxyShells[0].windowID, "0x01200007")
  }

  func testOpenGuestWindowProxyClosesTrackedShellWhenInventoryStopsVm()
    async throws
  {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    var stoppedVirtualMachine = virtualMachine
    stoppedVirtualMachine.status = .stopped
    stoppedVirtualMachine.uptime = "Not running"
    let window = GuestToolsWindowAction(
      id: "0x01200007",
      title: "Terminal",
      source: "wmctrl",
      focused: true,
      desktop: 0,
      pid: 4242,
      bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600)
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([stoppedVirtualMachine])
    )
    var openedPlans: [GuestWindowProxyPlan] = []
    var closedProxyShells: [(vmName: String, windowID: String)] = []
    let model = DashboardViewModel(
      client: client,
      openGuestWindowProxyShell: { plan, _ in
        openedPlans.append(plan)
      },
      closeGuestWindowProxyShell: { vmName, windowID in
        closedProxyShells.append((vmName, windowID))
      }
    )

    XCTAssertTrue(model.openGuestWindowProxy(for: window, in: virtualMachine))
    await model.load()
    client.listResult = .success([virtualMachine])
    await model.load()

    XCTAssertEqual(openedPlans.count, 1)
    XCTAssertEqual(closedProxyShells.count, 1)
    XCTAssertEqual(closedProxyShells[0].vmName, "Dev VM")
    XCTAssertEqual(closedProxyShells[0].windowID, "0x01200007")
  }

  func testOpenGuestWindowProxyClosesTrackedShellWhenInventoryRemovesVm()
    async throws
  {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    let window = GuestToolsWindowAction(
      id: "0x01200007",
      title: "Terminal",
      source: "wmctrl",
      focused: true,
      desktop: 0,
      pid: 4242,
      bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600)
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([])
    )
    var openedPlans: [GuestWindowProxyPlan] = []
    var closedProxyShells: [(vmName: String, windowID: String)] = []
    let model = DashboardViewModel(
      client: client,
      openGuestWindowProxyShell: { plan, _ in
        openedPlans.append(plan)
      },
      closeGuestWindowProxyShell: { vmName, windowID in
        closedProxyShells.append((vmName, windowID))
      }
    )

    XCTAssertTrue(model.openGuestWindowProxy(for: window, in: virtualMachine))
    await model.load()
    client.listResult = .success([virtualMachine])
    await model.load()

    XCTAssertEqual(openedPlans.count, 1)
    XCTAssertEqual(closedProxyShells.count, 1)
    XCTAssertEqual(closedProxyShells[0].vmName, "Dev VM")
    XCTAssertEqual(closedProxyShells[0].windowID, "0x01200007")
  }

  func testOpenGuestWindowProxyClosesTrackedShellWhenGuestReportsWindowClosed()
    async throws
  {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    let window = GuestToolsWindowAction(
      id: "0x01200007",
      title: "Terminal",
      source: "wmctrl",
      focused: true,
      desktop: 0,
      pid: 4242,
      bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600)
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(
        guestToolsStatus(
          vm: "Dev VM",
          window: GuestToolsWindowAction(
            id: "0x01200007",
            title: "Terminal",
            source: "wmctrl",
            focused: false,
            closed: true,
            desktop: 0,
            pid: 4242,
            bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600)
          )
        )
      )
    )
    var openedPlans: [GuestWindowProxyPlan] = []
    var closedProxyShells: [(vmName: String, windowID: String)] = []
    let model = DashboardViewModel(
      client: client,
      openGuestWindowProxyShell: { plan, _ in
        openedPlans.append(plan)
      },
      closeGuestWindowProxyShell: { vmName, windowID in
        closedProxyShells.append((vmName, windowID))
      }
    )

    XCTAssertTrue(model.openGuestWindowProxy(for: window, in: virtualMachine))
    await model.loadGuestToolsStatus(for: virtualMachine)
    client.guestToolsStatusResult = .success(
      guestToolsStatus(
        vm: "Dev VM",
        window: GuestToolsWindowAction(
          id: "0x01200007",
          title: "Terminal",
          source: "wmctrl",
          focused: true,
          desktop: 0,
          pid: 4242,
          bounds: GuestToolsWindowBounds(x: 80, y: 90, width: 1024, height: 768)
        )
      )
    )
    await model.loadGuestToolsStatus(for: virtualMachine)

    XCTAssertEqual(openedPlans.count, 1)
    XCTAssertEqual(closedProxyShells.count, 1)
    XCTAssertEqual(closedProxyShells[0].vmName, "Dev VM")
    XCTAssertEqual(closedProxyShells[0].windowID, "0x01200007")
  }

  func testOpenGuestWindowProxyAutoRefreshDispatchesWindowListAndRefreshesShell()
    async throws
  {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    let window = GuestToolsWindowAction(
      id: "0x01200007",
      title: "Terminal",
      source: "wmctrl",
      focused: true,
      desktop: 0,
      pid: 4242,
      bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600),
      cropFrameSummaryPath: "/tmp/crop-a.json"
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(
        guestToolsStatus(
          vm: "Dev VM",
          window: GuestToolsWindowAction(
            id: "0x01200007",
            title: "Terminal",
            source: "wmctrl",
            focused: true,
            desktop: 0,
            pid: 4242,
            bounds: GuestToolsWindowBounds(x: 55, y: 65, width: 1024, height: 768),
            cropFrameSummaryPath: "/tmp/crop-b.json"
          )
        )
      ),
      guestToolsCommandDispatchResult: .success(
        GuestToolsCommandDispatch(
          vm: "Dev VM",
          requestID: "proxy-refresh-windows-1",
          pendingCommands: 1
        )
      )
    )
    var openedPlans: [GuestWindowProxyPlan] = []
    let model = DashboardViewModel(
      client: client,
      openGuestWindowProxyShell: { plan, _ in
        openedPlans.append(plan)
      },
      guestWindowProxyRefreshIntervalNanoseconds: 1_000_000,
      guestWindowProxyRefreshStatusDelayNanoseconds: 0
    )

    XCTAssertTrue(model.openGuestWindowProxy(for: window, in: virtualMachine))
    XCTAssertEqual(
      model.guestWindowProxyStatus(for: virtualMachine),
      GuestWindowProxyStatus(
        trackedWindowCount: 1,
        cropBackedWindowCount: 1,
        trackedWindowSummaries: [
          "Terminal - 0x01200007 - 800x600 at 30,40 - crop"
        ],
        isAutoRefreshActive: true,
        isRefreshInFlight: false
      )
    )
    for _ in 0..<100 {
      if openedPlans.count >= 2
        && client.sentGuestToolsCommandRequests.contains(where: { $0.command == .listWindows })
      {
        break
      }
      try await Task.sleep(nanoseconds: 5_000_000)
    }

    XCTAssertTrue(client.sentGuestToolsCommandRequests.contains(where: { request in
      request.command == .listWindows
        && request.requestID?.hasPrefix("proxy-refresh-windows-") == true
    }))
    XCTAssertEqual(openedPlans.count, 2)
    XCTAssertEqual(openedPlans[1].cropFrameSummaryPath, "/tmp/crop-b.json")
    XCTAssertEqual(
      openedPlans[1].guestBounds,
      GuestToolsWindowBounds(x: 55, y: 65, width: 1024, height: 768)
    )
    XCTAssertEqual(model.guestWindowProxyStatus(for: virtualMachine).trackedWindowCount, 1)
    XCTAssertTrue(model.guestWindowProxyStatus(for: virtualMachine).isAutoRefreshActive)

    model.updateClient(client)
    XCTAssertEqual(model.guestWindowProxyStatus(for: virtualMachine), .idle)
  }

  func testOpenGuestWindowProxyClosesTrackedShellWhenAuthoritativeWindowListOmitsIt()
    async throws
  {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    let window = GuestToolsWindowAction(
      id: "0x01200007",
      title: "Terminal",
      source: "wmctrl",
      focused: true,
      desktop: 0,
      pid: 4242,
      bounds: GuestToolsWindowBounds(x: 30, y: 40, width: 800, height: 600)
    )
    let client = StubVirtualMachineClient(
      sourceTitle: "Mock inventory",
      listResult: .success([virtualMachine]),
      guestToolsStatusResult: .success(
        guestToolsStatus(vm: "Dev VM", windows: [])
      )
    )
    var openedPlans: [GuestWindowProxyPlan] = []
    var closedProxyShells: [(vmName: String, windowID: String)] = []
    let model = DashboardViewModel(
      client: client,
      openGuestWindowProxyShell: { plan, _ in
        openedPlans.append(plan)
      },
      closeGuestWindowProxyShell: { vmName, windowID in
        closedProxyShells.append((vmName, windowID))
      }
    )

    XCTAssertTrue(model.openGuestWindowProxy(for: window, in: virtualMachine))
    XCTAssertEqual(model.guestWindowProxyStatus(for: virtualMachine).trackedWindowCount, 1)
    await model.loadGuestToolsStatus(for: virtualMachine)
    client.guestToolsStatusResult = .success(
      guestToolsStatus(vm: "Dev VM", window: window)
    )
    await model.loadGuestToolsStatus(for: virtualMachine)

    XCTAssertEqual(openedPlans.count, 1)
    XCTAssertEqual(closedProxyShells.count, 1)
    XCTAssertEqual(closedProxyShells[0].vmName, "Dev VM")
    XCTAssertEqual(closedProxyShells[0].windowID, "0x01200007")
    XCTAssertEqual(model.guestWindowProxyStatus(for: virtualMachine), .idle)
  }

  func testOpenGuestWindowProxyRequiresBounds() {
    let virtualMachine = primaryActionPreflightVirtualMachine(status: .running)
    let window = GuestToolsWindowAction(
      id: "0x01200008",
      title: "Files",
      source: "wmctrl",
      focused: nil
    )
    var openedPlans: [GuestWindowProxyPlan] = []
    let model = DashboardViewModel(
      client: StubVirtualMachineClient(
        sourceTitle: "Mock inventory",
        listResult: .success([virtualMachine])
      ),
      openGuestWindowProxyShell: { plan, _ in
        openedPlans.append(plan)
      }
    )

    XCTAssertFalse(model.openGuestWindowProxy(for: window, in: virtualMachine))

    XCTAssertTrue(openedPlans.isEmpty)
    XCTAssertEqual(model.guestWindowProxyStatus(for: virtualMachine), .idle)
    XCTAssertEqual(
      model.alertMessage,
      "Guest window bounds are required before opening a proxy shell."
    )
  }

  private func guestToolsStatus(
    vm: String,
    window: GuestToolsWindowAction
  ) -> GuestToolsStatus {
    guestToolsStatus(vm: vm, windows: [window])
  }

  private func guestToolsStatus(
    vm: String,
    windows: [GuestToolsWindowAction]
  ) -> GuestToolsStatus {
    GuestToolsStatus(
      vm: vm,
      tools: "required",
      tokenCreatedAtUnix: 1_710_000_000,
      capabilities: [
        GuestToolsCapability(name: "windows", maxVersion: 1, enabledBy: "integration.windows")
      ],
      runtime: GuestToolsRuntime(
        connected: true,
        guestOS: "ubuntu",
        agentVersion: "0.1.0",
        capabilities: ["windows"],
        lastHeartbeatAtUnix: 1_710_000_060,
        guestIPAddresses: [],
        sharedFolders: [],
        metrics: nil,
        lastCommandResult: GuestToolsCommandResult(
          requestID: "windows-1",
          capability: "windows",
          ok: true,
          errorCode: nil,
          message: "listed windows",
          result: GuestToolsCommandPayload(
            value: .object([
              "windows": .array(windows.map { guestToolsJSON(window: $0) })
            ])
          ),
          metadata: nil,
          completedAtUnix: 1_710_000_062
        ),
        updatedAtUnix: 1_710_000_062
      )
    )
  }

  private func guestToolsJSON(window: GuestToolsWindowAction) -> GuestToolsJSONValue {
    var values: [String: GuestToolsJSONValue] = [
      "id": .string(window.id),
      "title": .string(window.title),
    ]
    if let source = window.source {
      values["source"] = .string(source)
    }
    if let focused = window.focused {
      values["focused"] = .bool(focused)
    }
    if let closed = window.closed {
      values["closed"] = .bool(closed)
    }
    if let desktop = window.desktop {
      values["desktop"] = .number("\(desktop)")
    }
    if let pid = window.pid {
      values["pid"] = .number("\(pid)")
    }
    if let bounds = window.bounds {
      values["bounds"] = .object([
        "x": .number("\(bounds.x)"),
        "y": .number("\(bounds.y)"),
        "width": .number("\(bounds.width)"),
        "height": .number("\(bounds.height)"),
      ])
    }
    if let cropFrameSummaryPath = window.cropFrameSummaryPath {
      values["window_crop_frame_summary_path"] = .string(cropFrameSummaryPath)
    }
    return .object(values)
  }

  private func primaryActionPreflightVirtualMachine(
    status: VirtualMachine.Status
  ) -> VirtualMachine {
    VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: status,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: status == .stopped ? "Not running" : "Launch failed",
      ipAddress: nil,
      lastStarted: nil,
      notes: "test"
    )
  }

  private func primaryActionPreflightRunnerStatus(
    launchReadiness: LaunchReadiness?
  ) -> RunnerStatus {
    RunnerStatus(
      engine: "apple-vz",
      pid: nil,
      command: ["bridgevm", "run", "Dev VM", "--backend", "apple-vz"],
      logPath: "logs/apple-vz.log",
      startedAtUnix: 1_710_000_200,
      dryRun: true,
      launchSpecPath: ".vmbridge/metadata/apple-vz-launch.json",
      launchReadiness: launchReadiness
    )
  }

  private func primaryActionPreflightReadinessReport(
    virtualMachine: VirtualMachine,
    runnerStatus: RunnerStatus?,
    preRunLaunchReadiness: LaunchReadiness? = nil
  ) -> VMReadinessReport {
    VMReadinessReport(
      vm: virtualMachine.name,
      mode: virtualMachine.mode,
      state: virtualMachine.status,
      metadataOnly: true,
      liveE2ERequired: true,
      evidenceRequirements: [],
      bootMedia: BootMediaStatus(
        vm: virtualMachine.name,
        entries: [
          BootMediaStatusEntry(
            kind: .installerImage,
            path: "installers/ubuntu-arm64.iso",
            exists: true,
            sizeBytes: 1_024,
            lastImport: nil,
            lastVerification: nil,
            lastDownloadPlan: nil,
            lastDownload: nil
          )
        ]
      ),
      bootMediaError: nil,
      snapshotChain: VMSnapshotChain(
        activeDisk: VMActiveDisk(
          source: "primary",
          snapshot: nil,
          path: "disks/root.qcow2",
          format: "qcow2",
          exists: true,
          activatedAtUnix: 1_710_000_100
        ),
        disks: []
      ),
      snapshotChainError: nil,
      runner: runnerStatus,
      runnerError: nil,
      preRunLaunchReadiness: preRunLaunchReadiness,
      blockers: [],
      notes: ["metadata-only report"]
    )
  }

  private func primaryActionPreflightDiskPreparation() -> DiskPreparation {
    DiskPreparation(
      path: "disks/root.qcow2",
      format: "qcow2",
      size: "64G",
      sizeBytes: 68_719_476_736,
      exists: true,
      created: false,
      createCommand: nil,
      preparedAtUnix: 1_710_000_300
    )
  }

  private static func guestToolsStatus(vm: String) -> GuestToolsStatus {
    GuestToolsStatus(
      vm: vm,
      tools: "metadata/guest-tools-token.json",
      tokenCreatedAtUnix: 1_710_000_100,
      capabilities: [
        GuestToolsCapability(
          name: "guest-tools-heartbeat",
          maxVersion: 1,
          enabledBy: "default"
        )
      ],
      approvedSharedFolders: [],
      runtime: nil
    )
  }
}

private final class StubVirtualMachineClient: VirtualMachineClient,
  VirtualMachineClientSourceProviding, VirtualMachineDisplayMetadataProviding
{
  var sourceTitle: String
  var allowsMutationsForCurrentInventory: Bool
  var listResult: Result<[VirtualMachine], Error>
  var displayStoreMetadataByID: [VirtualMachine.ID: EmbeddedDisplayLauncher.StoreMetadata]
  var templatesResult: Result<[BootTemplate], Error>
  var createResult: Result<VirtualMachine, Error>
  var cloneResult: Result<CloneVirtualMachineMetadata, Error>
  var exportResult: Result<VMExportMetadata, Error>
  var vmImportResult: Result<VMImportMetadata, Error>
  var readinessReportResult: Result<VMReadinessReport, Error>
  var bootMediaStatusResult: Result<BootMediaStatus, Error>
  var importResult: Result<BootMediaImportMetadata, Error>
  var verificationResult: Result<BootMediaVerificationMetadata, Error>
  var downloadPlanResult: Result<BootMediaDownloadPlanMetadata, Error>
  var downloadResult: Result<BootMediaDownloadResultMetadata, Error>
  var lifecyclePlanResult: Result<LifecyclePlan, Error>
  var openPortPlanResult: Result<OpenPortPlan, Error>
  var sshPlanResult: Result<SSHPlan, Error>
  var networkPlanResult: Result<NetworkPlan, Error>
  var portForwardListResult: Result<VMPortForwardList, Error>
  var guestToolsStatusResult: Result<GuestToolsStatus, Error>
  var guestToolsTokenResult: Result<GuestToolsToken, Error>
  var guestToolsLinuxCommandResults:
    [GuestToolsLinuxCommandTransport:
      Result<GuestToolsLinuxCommand, Error>]
  var guestToolsCommandDispatchResult: Result<GuestToolsCommandDispatch, Error>
  var sharedFolderListResult: Result<VMSharedFolderList, Error>
  var qmpStatusResult: Result<QMPStatus, Error>
  var qemuLaunchPlanResult: Result<QemuLaunchPlan, Error>
  var logViewResult: Result<VMLogView, Error>
  var prepareRunResult: Result<RunnerStatus, Error>
  var runnerStatusResult: Result<RunnerStatus?, Error>
  var runtimeControlResult: Result<RuntimeControlCommandResult, Error>
  var recommendationResult: Result<ModeRecommendation, Error>
  var snapshotPreflightStatusResult: Result<SnapshotPreflightStatus, Error>
  var snapshotsResult: Result<[VMSnapshot], Error>
  var snapshotChainResult: Result<VMSnapshotChain, Error>
  var snapshotCreationResult: Result<VMSnapshot, Error>
  var snapshotDiskCreationResult: Result<VMSnapshotDiskCreation, Error>
  var diskPreparationResult: Result<DiskPreparation, Error>
  var diskCreationResult: Result<VMDiskCreation, Error>
  var diskInspectionResult: Result<VMDiskInspection, Error>
  var diskVerificationResult: Result<VMDiskVerification, Error>
  var diskCompactionResult: Result<VMDiskCompaction, Error>
  var metadataRepairResult: Result<VMMetadataRepair, Error>
  var manifestMigrationResult: Result<VMManifestMigration, Error>
  var snapshotRestoreResult: Result<SnapshotRestoreResult, Error>
  var applicationConsistentSnapshotExecutionResult:
    Result<ApplicationConsistentSnapshotExecution, Error>
  var runtimeResourcePolicyResult: Result<RuntimeResourcePolicy, Error>
  var diagnosticBundleResult: Result<DiagnosticBundle, Error>
  var diagnosticBundleDelayNanos: UInt64
  var performanceBaselineResult: Result<PerformanceBaseline, Error>
  var performanceBaselineDelayNanos: UInt64
  var performanceSampleResult: Result<PerformanceSample, Error>
  var performanceSampleDelayNanos: UInt64
  var performResult: Result<VMActionResult, Error>
  var listDelayNanos: UInt64
  var readinessReportDelayNanos: UInt64
  var bootMediaStatusDelayNanos: UInt64
  var lifecyclePlanDelayNanos: UInt64
  var openPortPlanDelayNanos: UInt64
  var sshPlanDelayNanos: UInt64
  var networkPlanDelayNanos: UInt64
  var portForwardListDelayNanos: UInt64
  var sharedFolderListDelayNanos: UInt64
  var guestToolsStatusDelayNanos: UInt64
  var guestToolsTokenDelayNanos: UInt64
  var guestToolsLinuxCommandDelayNanos: UInt64
  var qmpStatusDelayNanos: UInt64
  var qemuLaunchPlanDelayNanos: UInt64
  var prepareRunDelayNanos: UInt64
  var performDelayNanos: UInt64
  var recommendationDelayNanos: UInt64
  var snapshotPreflightStatusDelayNanos: UInt64
  var snapshotsDelayNanos: UInt64
  var snapshotChainDelayNanos: UInt64
  var snapshotCreationDelayNanos: UInt64
  var snapshotDiskCreationDelayNanos: UInt64
  var snapshotRestoreDelayNanos: UInt64
  var applicationConsistentSnapshotExecutionDelayNanos: UInt64
  var diskPreparationDelayNanos: UInt64
  var diskCreationDelayNanos: UInt64
  var diskInspectionDelayNanos: UInt64
  var diskVerificationDelayNanos: UInt64
  var diskCompactionDelayNanos: UInt64
  var logViewDelayNanos: UInt64
  var deleteResult: Result<VMDeletionMetadata, Error>
  var createdRequests: [CreateVirtualMachineRequest] = []
  var inspectedReadinessReportIDs: [VirtualMachine.ID] = []
  var inspectedBootMediaStatusIDs: [VirtualMachine.ID] = []
  var inspectedLifecyclePlanRequests: [(action: LifecyclePlanAction, id: VirtualMachine.ID)] = []
  var inspectedOpenPortPlanRequests: [(guestPort: UInt16, scheme: String, id: VirtualMachine.ID)] =
    []
  var inspectedSSHPlanRequests: [(user: String, id: VirtualMachine.ID)] = []
  var loadedNetworkPlanIDs: [VirtualMachine.ID] = []
  var listedPortForwardIDs: [VirtualMachine.ID] = []
  var addedPortForwardRequests: [(host: UInt16, guest: UInt16, id: VirtualMachine.ID)] = []
  var removedPortForwardRequests: [(host: UInt16, guest: UInt16, id: VirtualMachine.ID)] = []
  var inspectedGuestToolsStatusIDs: [VirtualMachine.ID] = []
  var inspectedGuestToolsTokenIDs: [VirtualMachine.ID] = []
  private let inspectedGuestToolsLinuxCommandRequestLock = NSLock()
  var inspectedGuestToolsLinuxCommandRequests:
    [(transport: GuestToolsLinuxCommandTransport, id: VirtualMachine.ID)] = []
  var listedSharedFolderIDs: [VirtualMachine.ID] = []
  var inspectedQMPStatusIDs: [VirtualMachine.ID] = []
  var inspectedQemuLaunchPlanIDs: [VirtualMachine.ID] = []
  var viewedLogRequests: [(kind: VMLogKind, bytes: UInt64?, id: VirtualMachine.ID)] = []
  var preparedRunIDs: [VirtualMachine.ID] = []
  var inspectedRunnerStatusIDs: [VirtualMachine.ID] = []
  var sentRuntimeControlCommandRequests: [(command: String, id: VirtualMachine.ID)] = []
  var requestedModeChoices: [GuestChoice] = []
  var inspectedSnapshotPreflightStatusIDs: [VirtualMachine.ID] = []
  var listedSnapshotIDs: [VirtualMachine.ID] = []
  var inspectedSnapshotChainIDs: [VirtualMachine.ID] = []
  var createdSnapshotRequests:
    [(snapshotName: String, kind: VMSnapshotKind, id: VirtualMachine.ID)] = []
  var createdSnapshotDiskRequests: [(snapshotName: String, id: VirtualMachine.ID)] = []
  var preparedDiskIDs: [VirtualMachine.ID] = []
  var createdDiskIDs: [VirtualMachine.ID] = []
  var inspectedDiskIDs: [VirtualMachine.ID] = []
  var verifiedDiskIDs: [VirtualMachine.ID] = []
  var compactedDiskIDs: [VirtualMachine.ID] = []
  var repairedMetadataIDs: [VirtualMachine.ID] = []
  var migratedManifestRequests: [(id: VirtualMachine.ID, dryRun: Bool)] = []
  var restoredSnapshotRequests: [(snapshotName: String, id: VirtualMachine.ID)] = []
  var executedApplicationConsistentSnapshotRequests:
    [(snapshotName: String, freezeTimeoutMillis: UInt64?, id: VirtualMachine.ID)] = []
  var reappliedRuntimeResourceRequests:
    [(visibility: RuntimeResourceVisibility, id: VirtualMachine.ID)] = []
  var createdDiagnosticBundleRequests: [(output: String?, id: VirtualMachine.ID)] = []
  var createdPerformanceBaselineRequests: [(output: String?, id: VirtualMachine.ID)] = []
  var createdPerformanceSampleRequests:
    [(
      output: String?, artifactBytes: UInt64, iterations: UInt16, sync: Bool, id: VirtualMachine.ID
    )] =
      []
  var importedBootMediaRequests:
    [(sourcePath: String, kind: BootMediaStatusEntry.Kind?, id: VirtualMachine.ID)] = []
  var verifiedBootMediaRequests:
    [(expectedSHA256: String, kind: BootMediaStatusEntry.Kind?, id: VirtualMachine.ID)] = []
  var plannedBootMediaDownloadRequests:
    [(
      url: String, expectedSHA256: String?, kind: BootMediaStatusEntry.Kind?, id: VirtualMachine.ID
    )] = []
  var downloadedBootMediaRequests: [(kind: BootMediaStatusEntry.Kind?, id: VirtualMachine.ID)] = []
  var addedSharedFolderRequests:
    [(
      name: String, hostPath: String, readOnly: Bool, hostPathToken: String?,
      id: VirtualMachine.ID
    )] = []
  var removedSharedFolderRequests: [(shareName: String, id: VirtualMachine.ID)] = []
  var mountedApprovedSharedFolderRequests: [(shareName: String, id: VirtualMachine.ID)] = []
  var unmountedApprovedSharedFolderRequests: [(shareName: String, id: VirtualMachine.ID)] = []
  var sentGuestToolsCommandRequests:
    [(command: GuestToolsAgentCommand, requestID: String?, id: VirtualMachine.ID)] = []
  var clonedRequests: [(id: VirtualMachine.ID, newName: String, linked: Bool)] = []
  var deletedVMIDs: [VirtualMachine.ID] = []
  var exportedVMRequests: [(id: VirtualMachine.ID, output: String)] = []
  var importedVMRequests: [(input: String, name: String?)] = []
  var performedActionRequests: [(action: VirtualMachineAction, id: VirtualMachine.ID)] = []

  init(
    sourceTitle: String,
    allowsMutationsForCurrentInventory: Bool = true,
    listResult: Result<[VirtualMachine], Error>,
    displayStoreMetadataByID: [VirtualMachine.ID: EmbeddedDisplayLauncher.StoreMetadata] = [:],
    templatesResult: Result<[BootTemplate], Error> = .success([]),
    createResult: Result<VirtualMachine, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    cloneResult: Result<CloneVirtualMachineMetadata, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    exportResult: Result<VMExportMetadata, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    vmImportResult: Result<VMImportMetadata, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    readinessReportResult: Result<VMReadinessReport, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    bootMediaStatusResult: Result<BootMediaStatus, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    importResult: Result<BootMediaImportMetadata, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    verificationResult: Result<BootMediaVerificationMetadata, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    downloadPlanResult: Result<BootMediaDownloadPlanMetadata, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    downloadResult: Result<BootMediaDownloadResultMetadata, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    lifecyclePlanResult: Result<LifecyclePlan, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    openPortPlanResult: Result<OpenPortPlan, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    sshPlanResult: Result<SSHPlan, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    networkPlanResult: Result<NetworkPlan, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    portForwardListResult: Result<VMPortForwardList, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    guestToolsStatusResult: Result<GuestToolsStatus, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    guestToolsTokenResult: Result<GuestToolsToken, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    guestToolsLinuxCommandResults: [GuestToolsLinuxCommandTransport:
      Result<GuestToolsLinuxCommand, Error>] = [:],
    guestToolsCommandDispatchResult: Result<GuestToolsCommandDispatch, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    sharedFolderListResult: Result<VMSharedFolderList, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    qmpStatusResult: Result<QMPStatus, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    qemuLaunchPlanResult: Result<QemuLaunchPlan, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    logViewResult: Result<VMLogView, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    prepareRunResult: Result<RunnerStatus, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    runnerStatusResult: Result<RunnerStatus?, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    runtimeControlResult: Result<RuntimeControlCommandResult, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    recommendationResult: Result<ModeRecommendation, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    snapshotPreflightStatusResult: Result<SnapshotPreflightStatus, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    snapshotsResult: Result<[VMSnapshot], Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    snapshotChainResult: Result<VMSnapshotChain, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    snapshotCreationResult: Result<VMSnapshot, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    snapshotDiskCreationResult: Result<VMSnapshotDiskCreation, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    diskPreparationResult: Result<DiskPreparation, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    diskCreationResult: Result<VMDiskCreation, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    diskInspectionResult: Result<VMDiskInspection, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    diskVerificationResult: Result<VMDiskVerification, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    diskCompactionResult: Result<VMDiskCompaction, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    metadataRepairResult: Result<VMMetadataRepair, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    manifestMigrationResult: Result<VMManifestMigration, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    snapshotRestoreResult: Result<SnapshotRestoreResult, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    applicationConsistentSnapshotExecutionResult: Result<
      ApplicationConsistentSnapshotExecution, Error
    > = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    runtimeResourcePolicyResult: Result<RuntimeResourcePolicy, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    diagnosticBundleResult: Result<DiagnosticBundle, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    diagnosticBundleDelayNanos: UInt64 = 0,
    performanceBaselineResult: Result<PerformanceBaseline, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    performanceBaselineDelayNanos: UInt64 = 0,
    performanceSampleResult: Result<PerformanceSample, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    performanceSampleDelayNanos: UInt64 = 0,
    performResult: Result<VMActionResult, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound),
    listDelayNanos: UInt64 = 0,
    readinessReportDelayNanos: UInt64 = 0,
    bootMediaStatusDelayNanos: UInt64 = 0,
    lifecyclePlanDelayNanos: UInt64 = 0,
    openPortPlanDelayNanos: UInt64 = 0,
    sshPlanDelayNanos: UInt64 = 0,
    networkPlanDelayNanos: UInt64 = 0,
    portForwardListDelayNanos: UInt64 = 0,
    sharedFolderListDelayNanos: UInt64 = 0,
    guestToolsStatusDelayNanos: UInt64 = 0,
    guestToolsTokenDelayNanos: UInt64 = 0,
    guestToolsLinuxCommandDelayNanos: UInt64 = 0,
    qmpStatusDelayNanos: UInt64 = 0,
    qemuLaunchPlanDelayNanos: UInt64 = 0,
    prepareRunDelayNanos: UInt64 = 0,
    performDelayNanos: UInt64 = 0,
    recommendationDelayNanos: UInt64 = 0,
    snapshotPreflightStatusDelayNanos: UInt64 = 0,
    snapshotsDelayNanos: UInt64 = 0,
    snapshotChainDelayNanos: UInt64 = 0,
    snapshotCreationDelayNanos: UInt64 = 0,
    snapshotDiskCreationDelayNanos: UInt64 = 0,
    snapshotRestoreDelayNanos: UInt64 = 0,
    applicationConsistentSnapshotExecutionDelayNanos: UInt64 = 0,
    diskPreparationDelayNanos: UInt64 = 0,
    diskCreationDelayNanos: UInt64 = 0,
    diskInspectionDelayNanos: UInt64 = 0,
    diskVerificationDelayNanos: UInt64 = 0,
    diskCompactionDelayNanos: UInt64 = 0,
    logViewDelayNanos: UInt64 = 0,
    deleteResult: Result<VMDeletionMetadata, Error> = .failure(
      VirtualMachineClientError.virtualMachineNotFound)
  ) {
    self.sourceTitle = sourceTitle
    self.allowsMutationsForCurrentInventory = allowsMutationsForCurrentInventory
    self.listResult = listResult
    self.displayStoreMetadataByID = displayStoreMetadataByID
    self.templatesResult = templatesResult
    self.createResult = createResult
    self.cloneResult = cloneResult
    self.exportResult = exportResult
    self.vmImportResult = vmImportResult
    self.readinessReportResult = readinessReportResult
    self.bootMediaStatusResult = bootMediaStatusResult
    self.importResult = importResult
    self.verificationResult = verificationResult
    self.downloadPlanResult = downloadPlanResult
    self.downloadResult = downloadResult
    self.lifecyclePlanResult = lifecyclePlanResult
    self.openPortPlanResult = openPortPlanResult
    self.sshPlanResult = sshPlanResult
    self.networkPlanResult = networkPlanResult
    self.portForwardListResult = portForwardListResult
    self.guestToolsStatusResult = guestToolsStatusResult
    self.guestToolsTokenResult = guestToolsTokenResult
    self.guestToolsLinuxCommandResults = guestToolsLinuxCommandResults
    self.guestToolsCommandDispatchResult = guestToolsCommandDispatchResult
    self.sharedFolderListResult = sharedFolderListResult
    self.qmpStatusResult = qmpStatusResult
    self.qemuLaunchPlanResult = qemuLaunchPlanResult
    self.logViewResult = logViewResult
    self.prepareRunResult = prepareRunResult
    self.runnerStatusResult = runnerStatusResult
    self.runtimeControlResult = runtimeControlResult
    self.recommendationResult = recommendationResult
    self.snapshotPreflightStatusResult = snapshotPreflightStatusResult
    self.snapshotsResult = snapshotsResult
    self.snapshotChainResult = snapshotChainResult
    self.snapshotCreationResult = snapshotCreationResult
    self.snapshotDiskCreationResult = snapshotDiskCreationResult
    self.diskPreparationResult = diskPreparationResult
    self.diskCreationResult = diskCreationResult
    self.diskInspectionResult = diskInspectionResult
    self.diskVerificationResult = diskVerificationResult
    self.diskCompactionResult = diskCompactionResult
    self.metadataRepairResult = metadataRepairResult
    self.manifestMigrationResult = manifestMigrationResult
    self.snapshotRestoreResult = snapshotRestoreResult
    self.applicationConsistentSnapshotExecutionResult =
      applicationConsistentSnapshotExecutionResult
    self.runtimeResourcePolicyResult = runtimeResourcePolicyResult
    self.diagnosticBundleResult = diagnosticBundleResult
    self.diagnosticBundleDelayNanos = diagnosticBundleDelayNanos
    self.performanceBaselineResult = performanceBaselineResult
    self.performanceBaselineDelayNanos = performanceBaselineDelayNanos
    self.performanceSampleResult = performanceSampleResult
    self.performanceSampleDelayNanos = performanceSampleDelayNanos
    self.performResult = performResult
    self.listDelayNanos = listDelayNanos
    self.readinessReportDelayNanos = readinessReportDelayNanos
    self.bootMediaStatusDelayNanos = bootMediaStatusDelayNanos
    self.lifecyclePlanDelayNanos = lifecyclePlanDelayNanos
    self.openPortPlanDelayNanos = openPortPlanDelayNanos
    self.sshPlanDelayNanos = sshPlanDelayNanos
    self.networkPlanDelayNanos = networkPlanDelayNanos
    self.portForwardListDelayNanos = portForwardListDelayNanos
    self.sharedFolderListDelayNanos = sharedFolderListDelayNanos
    self.guestToolsStatusDelayNanos = guestToolsStatusDelayNanos
    self.guestToolsTokenDelayNanos = guestToolsTokenDelayNanos
    self.guestToolsLinuxCommandDelayNanos = guestToolsLinuxCommandDelayNanos
    self.qmpStatusDelayNanos = qmpStatusDelayNanos
    self.qemuLaunchPlanDelayNanos = qemuLaunchPlanDelayNanos
    self.prepareRunDelayNanos = prepareRunDelayNanos
    self.performDelayNanos = performDelayNanos
    self.recommendationDelayNanos = recommendationDelayNanos
    self.snapshotPreflightStatusDelayNanos = snapshotPreflightStatusDelayNanos
    self.snapshotsDelayNanos = snapshotsDelayNanos
    self.snapshotChainDelayNanos = snapshotChainDelayNanos
    self.snapshotCreationDelayNanos = snapshotCreationDelayNanos
    self.snapshotDiskCreationDelayNanos = snapshotDiskCreationDelayNanos
    self.snapshotRestoreDelayNanos = snapshotRestoreDelayNanos
    self.applicationConsistentSnapshotExecutionDelayNanos =
      applicationConsistentSnapshotExecutionDelayNanos
    self.diskPreparationDelayNanos = diskPreparationDelayNanos
    self.diskCreationDelayNanos = diskCreationDelayNanos
    self.diskInspectionDelayNanos = diskInspectionDelayNanos
    self.diskVerificationDelayNanos = diskVerificationDelayNanos
    self.diskCompactionDelayNanos = diskCompactionDelayNanos
    self.logViewDelayNanos = logViewDelayNanos
    self.deleteResult = deleteResult
  }

  func listVirtualMachines() async throws -> [VirtualMachine] {
    if listDelayNanos > 0 {
      try await Task.sleep(nanoseconds: listDelayNanos)
    }
    return try listResult.get()
  }

  func displayStoreMetadata(for id: VirtualMachine.ID) -> EmbeddedDisplayLauncher.StoreMetadata? {
    displayStoreMetadataByID[id]
  }

  func listBootTemplates() async throws -> [BootTemplate] {
    try templatesResult.get()
  }

  func inspectBootMediaStatus(on id: VirtualMachine.ID) async throws -> BootMediaStatus {
    inspectedBootMediaStatusIDs.append(id)
    if bootMediaStatusDelayNanos > 0 {
      try await Task.sleep(nanoseconds: bootMediaStatusDelayNanos)
    }
    return try bootMediaStatusResult.get()
  }

  func inspectReadinessReport(on id: VirtualMachine.ID) async throws -> VMReadinessReport {
    inspectedReadinessReportIDs.append(id)
    if readinessReportDelayNanos > 0 {
      try await Task.sleep(nanoseconds: readinessReportDelayNanos)
    }
    return try readinessReportResult.get()
  }

  func importBootMedia(
    sourcePath: String,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaImportMetadata {
    importedBootMediaRequests.append((sourcePath, kind, id))
    return try importResult.get()
  }

  func verifyBootMedia(
    expectedSHA256: String,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaVerificationMetadata {
    verifiedBootMediaRequests.append((expectedSHA256, kind, id))
    return try verificationResult.get()
  }

  func planBootMediaDownload(
    url: String,
    expectedSHA256: String?,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaDownloadPlanMetadata {
    plannedBootMediaDownloadRequests.append((url, expectedSHA256, kind, id))
    return try downloadPlanResult.get()
  }

  func downloadBootMedia(
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaDownloadResultMetadata {
    downloadedBootMediaRequests.append((kind, id))
    return try downloadResult.get()
  }

  func inspectLifecyclePlan(action: LifecyclePlanAction, on id: VirtualMachine.ID) async throws
    -> LifecyclePlan
  {
    inspectedLifecyclePlanRequests.append((action, id))
    if lifecyclePlanDelayNanos > 0 {
      try await Task.sleep(nanoseconds: lifecyclePlanDelayNanos)
    }
    return try lifecyclePlanResult.get()
  }

  func inspectOpenPortPlan(
    guestPort: UInt16,
    scheme: String,
    on id: VirtualMachine.ID
  ) async throws -> OpenPortPlan {
    inspectedOpenPortPlanRequests.append((guestPort, scheme, id))
    if openPortPlanDelayNanos > 0 {
      try await Task.sleep(nanoseconds: openPortPlanDelayNanos)
    }
    return try openPortPlanResult.get()
  }

  func inspectSSHPlan(user: String, on id: VirtualMachine.ID) async throws -> SSHPlan {
    inspectedSSHPlanRequests.append((user, id))
    if sshPlanDelayNanos > 0 {
      try await Task.sleep(nanoseconds: sshPlanDelayNanos)
    }
    return try sshPlanResult.get()
  }

  func inspectNetworkPlan(on id: VirtualMachine.ID) async throws -> NetworkPlan {
    loadedNetworkPlanIDs.append(id)
    if networkPlanDelayNanos > 0 {
      try await Task.sleep(nanoseconds: networkPlanDelayNanos)
    }
    return try networkPlanResult.get()
  }

  func listPortForwards(on id: VirtualMachine.ID) async throws -> VMPortForwardList {
    listedPortForwardIDs.append(id)
    if portForwardListDelayNanos > 0 {
      try await Task.sleep(nanoseconds: portForwardListDelayNanos)
    }
    return try portForwardListResult.get()
  }

  func addPortForward(host: UInt16, guest: UInt16, on id: VirtualMachine.ID) async throws
    -> VMPortForwardList
  {
    addedPortForwardRequests.append((host, guest, id))
    return try portForwardListResult.get()
  }

  func removePortForward(host: UInt16, guest: UInt16, on id: VirtualMachine.ID) async throws
    -> VMPortForwardList
  {
    removedPortForwardRequests.append((host, guest, id))
    return try portForwardListResult.get()
  }

  func inspectGuestToolsStatus(on id: VirtualMachine.ID) async throws -> GuestToolsStatus {
    inspectedGuestToolsStatusIDs.append(id)
    if guestToolsStatusDelayNanos > 0 {
      try await Task.sleep(nanoseconds: guestToolsStatusDelayNanos)
    }
    return try guestToolsStatusResult.get()
  }

  func inspectGuestToolsToken(on id: VirtualMachine.ID) async throws -> GuestToolsToken {
    inspectedGuestToolsTokenIDs.append(id)
    if guestToolsTokenDelayNanos > 0 {
      try await Task.sleep(nanoseconds: guestToolsTokenDelayNanos)
    }
    return try guestToolsTokenResult.get()
  }

  func inspectGuestToolsLinuxCommand(
    transport: GuestToolsLinuxCommandTransport,
    on id: VirtualMachine.ID
  ) async throws -> GuestToolsLinuxCommand {
    inspectedGuestToolsLinuxCommandRequestLock.withLock {
      inspectedGuestToolsLinuxCommandRequests.append((transport, id))
    }
    if guestToolsLinuxCommandDelayNanos > 0 {
      try await Task.sleep(nanoseconds: guestToolsLinuxCommandDelayNanos)
    }
    return try
      (guestToolsLinuxCommandResults[transport]
      ?? .failure(VirtualMachineClientError.virtualMachineNotFound)).get()
  }

  func listSharedFolders(on id: VirtualMachine.ID) async throws -> VMSharedFolderList {
    listedSharedFolderIDs.append(id)
    if sharedFolderListDelayNanos > 0 {
      try await Task.sleep(nanoseconds: sharedFolderListDelayNanos)
    }
    return try sharedFolderListResult.get()
  }

  func addSharedFolder(
    named shareName: String,
    hostPath: String,
    readOnly: Bool,
    hostPathToken: String?,
    on id: VirtualMachine.ID
  ) async throws -> VMSharedFolderList {
    addedSharedFolderRequests.append((shareName, hostPath, readOnly, hostPathToken, id))
    return try sharedFolderListResult.get()
  }

  func removeSharedFolder(named shareName: String, on id: VirtualMachine.ID) async throws
    -> VMSharedFolderList
  {
    removedSharedFolderRequests.append((shareName, id))
    return try sharedFolderListResult.get()
  }

  func mountApprovedSharedFolder(named shareName: String, on id: VirtualMachine.ID) async throws
    -> GuestToolsStatus?
  {
    mountedApprovedSharedFolderRequests.append((shareName, id))
    return try guestToolsStatusResult.get()
  }

  func unmountApprovedSharedFolder(named shareName: String, on id: VirtualMachine.ID) async throws
    -> GuestToolsStatus?
  {
    unmountedApprovedSharedFolderRequests.append((shareName, id))
    return try guestToolsStatusResult.get()
  }

  func sendGuestToolsCommand(
    _ command: GuestToolsAgentCommand,
    requestID: String?,
    on id: VirtualMachine.ID
  ) async throws -> GuestToolsCommandDispatch {
    sentGuestToolsCommandRequests.append((command, requestID, id))
    return try guestToolsCommandDispatchResult.get()
  }

  func inspectRunnerStatus(on id: VirtualMachine.ID) async throws -> RunnerStatus? {
    inspectedRunnerStatusIDs.append(id)
    return try runnerStatusResult.get()
  }

  func sendRuntimeControlCommand(_ command: String, on id: VirtualMachine.ID) async throws
    -> RuntimeControlCommandResult
  {
    sentRuntimeControlCommandRequests.append((command, id))
    return try runtimeControlResult.get()
  }

  func inspectQemuArgs(on id: VirtualMachine.ID) async throws -> QemuLaunchPlan {
    inspectedQemuLaunchPlanIDs.append(id)
    if qemuLaunchPlanDelayNanos > 0 {
      try await Task.sleep(nanoseconds: qemuLaunchPlanDelayNanos)
    }
    return try qemuLaunchPlanResult.get()
  }

  func prepareRun(on id: VirtualMachine.ID) async throws -> RunnerStatus {
    preparedRunIDs.append(id)
    if prepareRunDelayNanos > 0 {
      try await Task.sleep(nanoseconds: prepareRunDelayNanos)
    }
    return try prepareRunResult.get()
  }

  func recommendMode(for choice: GuestChoice) async throws -> ModeRecommendation {
    requestedModeChoices.append(choice)
    if recommendationDelayNanos > 0 {
      try await Task.sleep(nanoseconds: recommendationDelayNanos)
    }
    return try recommendationResult.get()
  }

  func inspectSnapshotPreflightStatus(on id: VirtualMachine.ID) async throws
    -> SnapshotPreflightStatus
  {
    inspectedSnapshotPreflightStatusIDs.append(id)
    if snapshotPreflightStatusDelayNanos > 0 {
      try await Task.sleep(nanoseconds: snapshotPreflightStatusDelayNanos)
    }
    return try snapshotPreflightStatusResult.get()
  }

  func listSnapshots(on id: VirtualMachine.ID) async throws -> [VMSnapshot] {
    listedSnapshotIDs.append(id)
    if snapshotsDelayNanos > 0 {
      try await Task.sleep(nanoseconds: snapshotsDelayNanos)
    }
    return try snapshotsResult.get()
  }

  func inspectSnapshotChain(on id: VirtualMachine.ID) async throws -> VMSnapshotChain {
    inspectedSnapshotChainIDs.append(id)
    if snapshotChainDelayNanos > 0 {
      try await Task.sleep(nanoseconds: snapshotChainDelayNanos)
    }
    return try snapshotChainResult.get()
  }

  func createSnapshot(named snapshotName: String, kind: VMSnapshotKind, on id: VirtualMachine.ID)
    async throws -> VMSnapshot
  {
    createdSnapshotRequests.append((snapshotName, kind, id))
    if snapshotCreationDelayNanos > 0 {
      try await Task.sleep(nanoseconds: snapshotCreationDelayNanos)
    }
    return try snapshotCreationResult.get()
  }

  func createSnapshotDisk(named snapshotName: String, on id: VirtualMachine.ID) async throws
    -> VMSnapshotDiskCreation
  {
    createdSnapshotDiskRequests.append((snapshotName, id))
    if snapshotDiskCreationDelayNanos > 0 {
      try await Task.sleep(nanoseconds: snapshotDiskCreationDelayNanos)
    }
    return try snapshotDiskCreationResult.get()
  }

  func preparePrimaryDisk(on id: VirtualMachine.ID) async throws -> DiskPreparation {
    preparedDiskIDs.append(id)
    if diskPreparationDelayNanos > 0 {
      try await Task.sleep(nanoseconds: diskPreparationDelayNanos)
    }
    return try diskPreparationResult.get()
  }

  func createPrimaryDisk(on id: VirtualMachine.ID) async throws -> VMDiskCreation {
    createdDiskIDs.append(id)
    if diskCreationDelayNanos > 0 {
      try await Task.sleep(nanoseconds: diskCreationDelayNanos)
    }
    return try diskCreationResult.get()
  }

  func inspectPrimaryDisk(on id: VirtualMachine.ID) async throws -> VMDiskInspection {
    inspectedDiskIDs.append(id)
    if diskInspectionDelayNanos > 0 {
      try await Task.sleep(nanoseconds: diskInspectionDelayNanos)
    }
    return try diskInspectionResult.get()
  }

  func verifyActiveDisk(on id: VirtualMachine.ID) async throws -> VMDiskVerification {
    verifiedDiskIDs.append(id)
    if diskVerificationDelayNanos > 0 {
      try await Task.sleep(nanoseconds: diskVerificationDelayNanos)
    }
    return try diskVerificationResult.get()
  }

  func compactActiveDisk(on id: VirtualMachine.ID) async throws -> VMDiskCompaction {
    compactedDiskIDs.append(id)
    if diskCompactionDelayNanos > 0 {
      try await Task.sleep(nanoseconds: diskCompactionDelayNanos)
    }
    return try diskCompactionResult.get()
  }

  func repairMetadata(on id: VirtualMachine.ID) async throws -> VMMetadataRepair {
    repairedMetadataIDs.append(id)
    return try metadataRepairResult.get()
  }

  func migrateManifest(on id: VirtualMachine.ID, dryRun: Bool) async throws -> VMManifestMigration {
    migratedManifestRequests.append((id, dryRun))
    return try manifestMigrationResult.get()
  }

  func restoreSnapshot(named snapshotName: String, on id: VirtualMachine.ID) async throws
    -> SnapshotRestoreResult
  {
    restoredSnapshotRequests.append((snapshotName, id))
    if snapshotRestoreDelayNanos > 0 {
      try await Task.sleep(nanoseconds: snapshotRestoreDelayNanos)
    }
    return try snapshotRestoreResult.get()
  }

  func executeApplicationConsistentSnapshot(
    named snapshotName: String,
    freezeTimeoutMillis: UInt64?,
    on id: VirtualMachine.ID
  ) async throws -> ApplicationConsistentSnapshotExecution {
    executedApplicationConsistentSnapshotRequests.append((snapshotName, freezeTimeoutMillis, id))
    if applicationConsistentSnapshotExecutionDelayNanos > 0 {
      try await Task.sleep(nanoseconds: applicationConsistentSnapshotExecutionDelayNanos)
    }
    return try applicationConsistentSnapshotExecutionResult.get()
  }

  func reapplyRuntimeResources(
    visibility: RuntimeResourceVisibility,
    on id: VirtualMachine.ID
  ) async throws -> RuntimeResourcePolicy {
    reappliedRuntimeResourceRequests.append((visibility, id))
    return try runtimeResourcePolicyResult.get()
  }

  func createDiagnosticBundle(output: String?, on id: VirtualMachine.ID) async throws
    -> DiagnosticBundle
  {
    createdDiagnosticBundleRequests.append((output, id))
    if diagnosticBundleDelayNanos > 0 {
      try await Task.sleep(nanoseconds: diagnosticBundleDelayNanos)
    }
    return try diagnosticBundleResult.get()
  }

  func createPerformanceBaseline(output: String?, on id: VirtualMachine.ID) async throws
    -> PerformanceBaseline
  {
    createdPerformanceBaselineRequests.append((output, id))
    if performanceBaselineDelayNanos > 0 {
      try await Task.sleep(nanoseconds: performanceBaselineDelayNanos)
    }
    return try performanceBaselineResult.get()
  }

  func createPerformanceSample(
    output: String?,
    artifactBytes: UInt64,
    iterations: UInt16,
    sync: Bool,
    on id: VirtualMachine.ID
  ) async throws -> PerformanceSample {
    createdPerformanceSampleRequests.append((output, artifactBytes, iterations, sync, id))
    if performanceSampleDelayNanos > 0 {
      try await Task.sleep(nanoseconds: performanceSampleDelayNanos)
    }
    return try performanceSampleResult.get()
  }

  func inspectQMPStatus(on id: VirtualMachine.ID) async throws -> QMPStatus {
    inspectedQMPStatusIDs.append(id)
    if qmpStatusDelayNanos > 0 {
      try await Task.sleep(nanoseconds: qmpStatusDelayNanos)
    }
    return try qmpStatusResult.get()
  }

  func viewLogs(kind: VMLogKind, bytes: UInt64?, on id: VirtualMachine.ID) async throws
    -> VMLogView
  {
    viewedLogRequests.append((kind, bytes, id))
    if logViewDelayNanos > 0 {
      try await Task.sleep(nanoseconds: logViewDelayNanos)
    }
    return try logViewResult.get()
  }

  func createVirtualMachine(_ request: CreateVirtualMachineRequest) async throws -> VirtualMachine {
    createdRequests.append(request)
    return try createResult.get()
  }

  func cloneVirtualMachine(on id: VirtualMachine.ID, newName: String, linked: Bool) async throws
    -> CloneVirtualMachineMetadata
  {
    clonedRequests.append((id, newName, linked))
    return try cloneResult.get()
  }

  func deleteVirtualMachine(on id: VirtualMachine.ID) async throws -> VMDeletionMetadata {
    deletedVMIDs.append(id)
    return try deleteResult.get()
  }

  func exportVirtualMachine(on id: VirtualMachine.ID, output: String) async throws
    -> VMExportMetadata
  {
    exportedVMRequests.append((id, output))
    return try exportResult.get()
  }

  func importVirtualMachine(input: String, name: String?) async throws -> VMImportMetadata {
    importedVMRequests.append((input, name))
    return try vmImportResult.get()
  }

  func perform(_ action: VirtualMachineAction, on id: VirtualMachine.ID) async throws
    -> VMActionResult
  {
    performedActionRequests.append((action, id))
    if performDelayNanos > 0 {
      try await Task.sleep(nanoseconds: performDelayNanos)
    }
    return try performResult.get()
  }
}

private enum TestRefreshError: LocalizedError {
  case offline

  var errorDescription: String? {
    "Offline"
  }
}
