import AppKit
import SwiftUI

struct VMDetailView: View {
  var virtualMachine: VirtualMachine
  var isWorking: Bool
  var isCloning: Bool
  var isOpeningConsole: Bool
  var readinessReport: VMReadinessReport?
  var isLoadingReadinessReport: Bool
  var readinessReportError: String?
  var bootMediaStatus: BootMediaStatus?
  var isLoadingBootMediaStatus: Bool
  var isImportingBootMedia: Bool
  var isVerifyingBootMedia: Bool
  var isPlanningBootMediaDownload: Bool
  var isDownloadingBootMedia: Bool
  var bootMediaStatusError: String?
  var guestToolsStatus: GuestToolsStatus?
  var guestWindowProxyStatus: GuestWindowProxyStatus = .idle
  var guestToolsProvisioning: GuestToolsProvisioning? = nil
  var isLoadingGuestToolsStatus: Bool
  var isSendingGuestToolsCommand: Bool
  var guestToolsStatusError: String?
  var guestToolsProvisioningError: String? = nil
  var sharedFolderList: VMSharedFolderList? = nil
  var isLoadingSharedFolders = false
  var isAddingSharedFolder = false
  var isRemovingSharedFolder = false
  var sharedFolderError: String? = nil
  var runnerStatus: RunnerStatus?
  var isLoadingRunnerStatus: Bool
  var runnerStatusError: String?
  var runtimeControlResult: RuntimeControlCommandResult? = nil
  var isSendingRuntimeControl = false
  var runtimeControlError: String? = nil
  var snapshotPreflightStatus: SnapshotPreflightStatus?
  var isLoadingSnapshotPreflightStatus: Bool
  var snapshotPreflightStatusError: String?
  var snapshots: [VMSnapshot] = []
  var isLoadingSnapshots = false
  var snapshotError: String? = nil
  var snapshotChain: VMSnapshotChain? = nil
  var isLoadingSnapshotChain = false
  var snapshotChainError: String? = nil
  var diskPreparation: DiskPreparation? = nil
  var isPreparingDisk = false
  var diskPreparationError: String? = nil
  var diskCreation: VMDiskCreation? = nil
  var isCreatingDisk = false
  var diskCreationError: String? = nil
  var diskInspection: VMDiskInspection? = nil
  var isInspectingDisk = false
  var diskInspectionError: String? = nil
  var diskVerification: VMDiskVerification? = nil
  var isVerifyingDisk = false
  var diskVerificationError: String? = nil
  var diskCompaction: VMDiskCompaction? = nil
  var isCompactingDisk = false
  var diskCompactionError: String? = nil
  var metadataRepair: VMMetadataRepair? = nil
  var isRepairingMetadata = false
  var metadataRepairError: String? = nil
  var manifestMigration: VMManifestMigration? = nil
  var isCheckingManifestMigration = false
  var manifestMigrationError: String? = nil
  var snapshotRestoreResult: SnapshotRestoreResult? = nil
  var isRestoringSnapshot = false
  var snapshotRestoreError: String? = nil
  var snapshotCreation: VMSnapshot? = nil
  var isCreatingSnapshot = false
  var snapshotCreationError: String? = nil
  var snapshotDiskCreation: VMSnapshotDiskCreation? = nil
  var isCreatingSnapshotDisk = false
  var snapshotDiskCreationError: String? = nil
  var applicationConsistentSnapshotExecution: ApplicationConsistentSnapshotExecution? = nil
  var isExecutingApplicationConsistentSnapshot = false
  var applicationConsistentSnapshotExecutionError: String? = nil
  var runtimeResourcePolicy: RuntimeResourcePolicy? = nil
  var isReapplyingRuntimeResources = false
  var runtimeResourcePolicyError: String? = nil
  var vmExport: VMExportMetadata? = nil
  var isExportingVirtualMachine = false
  var vmExportError: String? = nil
  var lastVMImport: VMImportMetadata? = nil
  var isImportingVirtualMachine = false
  var vmImportError: String? = nil
  var diagnosticBundle: DiagnosticBundle? = nil
  var isCreatingDiagnosticBundle = false
  var diagnosticBundleError: String? = nil
  var performanceBaseline: PerformanceBaseline? = nil
  var isCreatingPerformanceBaseline = false
  var performanceBaselineError: String? = nil
  var performanceSample: PerformanceSample? = nil
  var isCreatingPerformanceSample = false
  var performanceSampleError: String? = nil
  var qmpStatus: QMPStatus?
  var qmpStatusError: String?
  var qemuLaunchPlan: QemuLaunchPlan?
  var isLoadingQemuLaunchPlan = false
  var qemuLaunchPlanError: String?
  var qemuLog: VMLogView?
  var serialLog: VMLogView?
  var isLoadingLog: Bool
  var logViewError: String?
  var lifecycleActions: [LifecycleActionOption]
  var lifecyclePlan: LifecyclePlan?
  var isLoadingLifecyclePlan: Bool
  var lifecyclePlanError: String?
  var portForwardList: VMPortForwardList? = nil
  var isLoadingPortForwards = false
  var isAddingPortForward = false
  var isRemovingPortForward = false
  var portForwardError: String? = nil
  var openPortPlan: OpenPortPlan?
  var isLoadingOpenPortPlan: Bool
  var openPortPlanError: String?
  var sshPlan: SSHPlan?
  var isLoadingSSHPlan: Bool
  var sshPlanError: String?
  var networkPlan: NetworkPlan?
  var isLoadingNetworkPlan: Bool
  var networkPlanError: String?
  var onPrimaryAction: () async -> Void
  var onClone: () -> Void
  var onOpenConsole: () async -> Bool
  var onShowDisplay: (String, String) async -> Void = { _, _ in }
  var onStop: () async -> Void
  var onRestart: () async -> Void
  var onPerformLifecycleAction: (VirtualMachineAction) async -> Void
  var onInspectLifecyclePlan: (LifecyclePlanAction) async -> Void
  var onRefreshPortForwards: () async -> Void = {}
  var onAddPortForward: (String, String) async -> Bool = { _, _ in false }
  var onRemovePortForward: (UInt16, UInt16) async -> Bool = { _, _ in false }
  var onInspectOpenPortPlan: (String, String) async -> Bool
  var onInspectSSHPlan: (String) async -> Bool
  var onRefreshNetworkPlan: () async -> Void
  var onRefreshBootMediaStatus: () async -> Void
  var onImportBootMedia: (String, BootMediaStatusEntry.Kind?) async -> Bool
  var onVerifyBootMedia: (String, BootMediaStatusEntry.Kind?) async -> Bool
  var onPlanBootMediaDownload: (String, String?, BootMediaStatusEntry.Kind?) async -> Bool
  var onDownloadBootMedia: (BootMediaStatusEntry.Kind?) async -> Bool
  var onRefreshGuestToolsStatus: () async -> Void
  var onRefreshSharedFolders: () async -> Void = {}
  var onAddSharedFolder: (String, String, Bool, String) async -> Bool = { _, _, _, _ in false }
  var onRemoveSharedFolder: (String) async -> Bool = { _ in false }
  var onMountApprovedSharedFolder: (String) async -> Bool
  var onUnmountApprovedSharedFolder: (String) async -> Bool
  var onSendGuestToolsCommand: (GuestToolsAgentCommand) async -> Bool
  var onSyncGuestTime: () async -> Bool = { false }
  var onSetClipboardText: (String) async -> Bool
  var onResizeDisplay: (String, String, String) async -> Bool
  var onLaunchApplication: (String) async -> Bool
  var onFocusWindow: (String) async -> Bool
  var onCloseWindow: (String) async -> Bool
  var onOpenWindowProxy: (GuestToolsWindowAction) async -> Bool = { _ in false }
  var onCloseWindowProxies: () -> Void = {}
  var onSendInlineFileDrop: (String, String) async -> Bool
  var onPrepareRun: () async -> Bool = { false }
  var onRefreshRunnerStatus: () async -> Void
  var onRuntimeControlStatus: () async -> Bool = { false }
  var onRuntimeControlStopDisplay: () async -> Bool = { false }
  var onRuntimeControlPolicy: () async -> Bool = { false }
  var onRuntimeControlPacing: () async -> Bool = { false }
  var onRefreshSnapshotPreflightStatus: () async -> Void
  var onRefreshSnapshots: () async -> Void = {}
  var onRefreshSnapshotChain: () async -> Void = {}
  var onPreparePrimaryDisk: () async -> Bool = { false }
  var onCreatePrimaryDisk: () async -> Bool = { false }
  var onInspectPrimaryDisk: () async -> Bool = { false }
  var onVerifyActiveDisk: () async -> Bool = { false }
  var onCompactActiveDisk: () async -> Bool = { false }
  var onRepairMetadata: () async -> Bool = { false }
  var onCheckManifestMigration: () async -> Bool = { false }
  var onRestoreSnapshot: (String) async -> Bool = { _ in false }
  var onCreateSnapshot: (String, VMSnapshotKind) async -> Bool = { _, _ in false }
  var onCreateSnapshotDisk: (String) async -> Bool = { _ in false }
  var onExecuteApplicationConsistentSnapshot: (String, UInt64?) async -> Bool = { _, _ in
    false
  }
  var onReapplyRuntimeResources: (RuntimeResourceVisibility) async -> Bool = { _ in false }
  var onExportVirtualMachine: (String) async -> Bool = { _ in false }
  var onImportVirtualMachine: (String, String) async -> Bool = { _, _ in false }
  var onCreateDiagnosticBundle: (String) async -> Bool = { _ in false }
  var onCreatePerformanceBaseline: (String) async -> Bool = { _ in false }
  var onCreatePerformanceSample: (String, String, String, Bool) async -> Bool = { _, _, _, _ in
    false
  }
  var onRefreshQemuLaunchPlan: () async -> Void = {}
  var onLoadLog: (VMLogKind) async -> Void

  var body: some View {
    VStack(spacing: 0) {
      DetailToolbar(
        virtualMachine: virtualMachine,
        isWorking: isWorking,
        isCloning: isCloning,
        isOpeningConsole: isOpeningConsole,
        qemuLaunchPlan: qemuLaunchPlan,
        onPrimaryAction: onPrimaryAction,
        onClone: onClone,
        onOpenConsole: onOpenConsole,
        onStop: onStop,
        onRestart: onRestart
      )

      Divider()

      ScrollView {
        VStack(alignment: .leading, spacing: 24) {
          ConsoleDiagnosticsPanel(
            virtualMachine: virtualMachine,
            qmpStatus: qmpStatus,
            qmpStatusError: qmpStatusError,
            qemuLaunchPlan: qemuLaunchPlan,
            qemuLaunchPlanError: qemuLaunchPlanError,
            qemuLog: qemuLog,
            serialLog: serialLog,
            isOpeningConsole: isOpeningConsole,
            isLoadingLog: isLoadingLog,
            logViewError: logViewError,
            onOpenConsole: onOpenConsole,
            onLoadLog: onLoadLog,
            onShowDisplay: onShowDisplay
          )

          VMReadinessNextActionPanel(
            virtualMachine: virtualMachine,
            readinessReport: readinessReport,
            isLoadingReadinessReport: isLoadingReadinessReport,
            readinessReportError: readinessReportError,
            bootMediaStatus: bootMediaStatus,
            isLoadingBootMediaStatus: isLoadingBootMediaStatus,
            bootMediaStatusError: bootMediaStatusError,
            guestToolsStatus: guestToolsStatus,
            isLoadingGuestToolsStatus: isLoadingGuestToolsStatus,
            guestToolsStatusError: guestToolsStatusError,
            runnerStatus: runnerStatus,
            isLoadingRunnerStatus: isLoadingRunnerStatus,
            runnerStatusError: runnerStatusError,
            snapshotPreflightStatus: snapshotPreflightStatus,
            isLoadingSnapshotPreflightStatus: isLoadingSnapshotPreflightStatus,
            snapshotPreflightStatusError: snapshotPreflightStatusError,
            snapshotChain: snapshotChain,
            snapshotChainError: snapshotChainError,
            diskPreparation: diskPreparation,
            diskCreation: diskCreation,
            diskInspection: diskInspection,
            diskVerification: diskVerification,
            qmpStatus: qmpStatus,
            qmpStatusError: qmpStatusError,
            isWorking: isWorking,
            isOpeningConsole: isOpeningConsole,
            onPrimaryAction: onPrimaryAction,
            onOpenConsole: onOpenConsole,
            onRefreshBootMediaStatus: onRefreshBootMediaStatus,
            onRefreshGuestToolsStatus: onRefreshGuestToolsStatus,
            onPrepareRun: onPrepareRun,
            onPreparePrimaryDisk: onPreparePrimaryDisk,
            onRefreshRunnerStatus: onRefreshRunnerStatus
          )

          LazyVGrid(columns: [GridItem(.adaptive(minimum: 220), spacing: 14)], spacing: 14) {
            MetricCard(title: "Guest", value: virtualMachine.guest, systemImage: "cpu")
            MetricCard(
              title: "Mode", value: virtualMachine.mode.title, systemImage: "bolt.horizontal")
            MetricCard(title: "Resources", value: resourcesText, systemImage: "memorychip")
            MetricCard(title: "Network", value: networkText, systemImage: "network")
          }

          VStack(alignment: .leading, spacing: 10) {
            Text("Activity")
              .font(.headline)

            TimelineRow(title: "Status", detail: virtualMachine.status.title)
            TimelineRow(title: "Uptime", detail: virtualMachine.uptime)
            TimelineRow(title: "Last Started", detail: lastStartedText)
            TimelineRow(title: "Notes", detail: virtualMachine.notes)
          }

          LifecycleControlsPanel(
            virtualMachine: virtualMachine,
            actions: lifecycleActions,
            isWorking: isWorking,
            plan: lifecyclePlan,
            isLoadingPlan: isLoadingLifecyclePlan,
            planError: lifecyclePlanError,
            onInspectPlan: onInspectLifecyclePlan,
            onPerform: onPerformLifecycleAction
          )

          PortForwardManifestPanel(
            list: portForwardList,
            isLoading: isLoadingPortForwards,
            isAdding: isAddingPortForward,
            isRemoving: isRemovingPortForward,
            error: portForwardError,
            onRefresh: onRefreshPortForwards,
            onAdd: onAddPortForward,
            onRemove: onRemovePortForward
          )

          OpenPortPlanPanel(
            plan: openPortPlan,
            isLoading: isLoadingOpenPortPlan,
            error: openPortPlanError,
            onInspect: onInspectOpenPortPlan
          )

          SSHPlanPanel(
            plan: sshPlan,
            isLoading: isLoadingSSHPlan,
            error: sshPlanError,
            onInspect: onInspectSSHPlan
          )

          NetworkPlanPanel(
            plan: networkPlan,
            isLoading: isLoadingNetworkPlan,
            error: networkPlanError,
            onRefresh: onRefreshNetworkPlan
          )

          BootMediaStatusPanel(
            status: bootMediaStatus,
            isLoading: isLoadingBootMediaStatus,
            isImporting: isImportingBootMedia,
            isVerifying: isVerifyingBootMedia,
            isPlanningDownload: isPlanningBootMediaDownload,
            isDownloading: isDownloadingBootMedia,
            error: bootMediaStatusError,
            onRefresh: onRefreshBootMediaStatus,
            onImport: onImportBootMedia,
            onVerify: onVerifyBootMedia,
            onPlanDownload: onPlanBootMediaDownload,
            onDownload: onDownloadBootMedia
          )

          SnapshotPreflightPanel(
            status: snapshotPreflightStatus,
            isLoading: isLoadingSnapshotPreflightStatus,
            error: snapshotPreflightStatusError,
            execution: applicationConsistentSnapshotExecution,
            isExecuting: isExecutingApplicationConsistentSnapshot,
            executionError: applicationConsistentSnapshotExecutionError,
            onRefresh: onRefreshSnapshotPreflightStatus,
            onExecute: onExecuteApplicationConsistentSnapshot
          )

          RunnerStatusPanel(
            status: runnerStatus,
            preRunLaunchReadiness: readinessReport?.preRunLaunchReadiness,
            isLoading: isLoadingRunnerStatus,
            error: runnerStatusError,
            runtimeControlResult: runtimeControlResult,
            isSendingRuntimeControl: isSendingRuntimeControl,
            runtimeControlError: runtimeControlError,
            onPrepare: onPrepareRun,
            onRefresh: onRefreshRunnerStatus,
            onRuntimeControlStatus: onRuntimeControlStatus,
            onRuntimeControlStopDisplay: onRuntimeControlStopDisplay,
            onRuntimeControlPolicy: onRuntimeControlPolicy,
            onRuntimeControlPacing: onRuntimeControlPacing
          )

          if virtualMachine.mode == .fast {
            RuntimeResourcePolicyPanel(
              policy: runtimeResourcePolicy,
              isApplying: isReapplyingRuntimeResources,
              error: runtimeResourcePolicyError,
              onReapply: onReapplyRuntimeResources
            )
          }

          QemuLaunchPlanPanel(
            plan: qemuLaunchPlan,
            isLoading: isLoadingQemuLaunchPlan,
            error: qemuLaunchPlanError,
            onRefresh: onRefreshQemuLaunchPlan
          )

          SnapshotMetadataPanel(
            snapshots: snapshots,
            isLoading: isLoadingSnapshots,
            error: snapshotError,
            chain: snapshotChain,
            isLoadingChain: isLoadingSnapshotChain,
            chainError: snapshotChainError,
            restoreResult: snapshotRestoreResult,
            isRestoring: isRestoringSnapshot,
            restoreError: snapshotRestoreError,
            snapshotCreation: snapshotCreation,
            isCreatingSnapshot: isCreatingSnapshot,
            snapshotCreationError: snapshotCreationError,
            diskCreation: snapshotDiskCreation,
            isCreatingDisk: isCreatingSnapshotDisk,
            diskCreationError: snapshotDiskCreationError,
            onRefresh: onRefreshSnapshots,
            onRefreshChain: onRefreshSnapshotChain,
            onRestore: onRestoreSnapshot,
            onCreateSnapshot: onCreateSnapshot,
            onCreateDisk: onCreateSnapshotDisk
          )

          StorageMaintenancePanel(
            preparation: diskPreparation,
            isPreparing: isPreparingDisk,
            preparationError: diskPreparationError,
            creation: diskCreation,
            isCreating: isCreatingDisk,
            creationError: diskCreationError,
            inspection: diskInspection,
            isInspecting: isInspectingDisk,
            inspectionError: diskInspectionError,
            verification: diskVerification,
            isVerifying: isVerifyingDisk,
            verificationError: diskVerificationError,
            compaction: diskCompaction,
            isCompacting: isCompactingDisk,
            compactionError: diskCompactionError,
            onPrepare: onPreparePrimaryDisk,
            onCreate: onCreatePrimaryDisk,
            onInspect: onInspectPrimaryDisk,
            onVerify: onVerifyActiveDisk,
            onCompact: onCompactActiveDisk
          )

          MetadataRepairPanel(
            repair: metadataRepair,
            isRepairing: isRepairingMetadata,
            error: metadataRepairError,
            migration: manifestMigration,
            isCheckingMigration: isCheckingManifestMigration,
            migrationError: manifestMigrationError,
            onRepair: onRepairMetadata,
            onCheckMigration: onCheckManifestMigration
          )

          PortableBundlePanel(
            export: vmExport,
            isExporting: isExportingVirtualMachine,
            exportError: vmExportError,
            lastImport: lastVMImport,
            isImporting: isImportingVirtualMachine,
            importError: vmImportError,
            onExport: onExportVirtualMachine,
            onImport: onImportVirtualMachine
          )

          DiagnosticsPerformancePanel(
            diagnosticBundle: diagnosticBundle,
            isCreatingDiagnosticBundle: isCreatingDiagnosticBundle,
            diagnosticBundleError: diagnosticBundleError,
            performanceBaseline: performanceBaseline,
            isCreatingPerformanceBaseline: isCreatingPerformanceBaseline,
            performanceBaselineError: performanceBaselineError,
            performanceSample: performanceSample,
            isCreatingPerformanceSample: isCreatingPerformanceSample,
            performanceSampleError: performanceSampleError,
            onCreateDiagnosticBundle: onCreateDiagnosticBundle,
            onCreatePerformanceBaseline: onCreatePerformanceBaseline,
            onCreatePerformanceSample: onCreatePerformanceSample
          )

          LogViewerPanel(
            qemuLog: qemuLog,
            serialLog: serialLog,
            isLoading: isLoadingLog,
            error: logViewError,
            onLoad: onLoadLog
          )

          SharedFolderManifestPanel(
            list: sharedFolderList,
            isLoading: isLoadingSharedFolders,
            isAdding: isAddingSharedFolder,
            isRemoving: isRemovingSharedFolder,
            error: sharedFolderError,
            onRefresh: onRefreshSharedFolders,
            onAdd: onAddSharedFolder,
            onRemove: onRemoveSharedFolder
          )

          GuestToolsStatusPanel(
            status: guestToolsStatus,
            guestWindowProxyStatus: guestWindowProxyStatus,
            provisioning: guestToolsProvisioning,
            isLoading: isLoadingGuestToolsStatus,
            isSendingCommand: isSendingGuestToolsCommand,
            error: guestToolsStatusError,
            provisioningError: guestToolsProvisioningError,
            onRefresh: onRefreshGuestToolsStatus,
            onMountApprovedSharedFolder: onMountApprovedSharedFolder,
            onUnmountApprovedSharedFolder: onUnmountApprovedSharedFolder,
            onSendCommand: onSendGuestToolsCommand,
            onSyncGuestTime: onSyncGuestTime,
            onSetClipboardText: onSetClipboardText,
            onResizeDisplay: onResizeDisplay,
            onLaunchApplication: onLaunchApplication,
            onFocusWindow: onFocusWindow,
            onCloseWindow: onCloseWindow,
            onOpenWindowProxy: onOpenWindowProxy,
            onCloseWindowProxies: onCloseWindowProxies,
            onSendInlineFileDrop: onSendInlineFileDrop
          )
        }
        .padding(24)
        .frame(maxWidth: .infinity, alignment: .leading)
      }
    }
    .background(Color(nsColor: .textBackgroundColor))
  }

  private var resourcesText: String {
    "\(virtualMachine.resources.cpuCount) CPU / \(virtualMachine.resources.memoryGB) GB / \(virtualMachine.resources.diskGB) GB"
  }

  private var networkText: String {
    guestToolsStatus?.primaryIPAddress ?? virtualMachine.ipAddress ?? "Disconnected"
  }

  private var lastStartedText: String {
    guard let lastStarted = virtualMachine.lastStarted else {
      return "Never"
    }

    return lastStarted.formatted(date: .abbreviated, time: .shortened)
  }
}

private struct DetailToolbar: View {
  var virtualMachine: VirtualMachine
  var isWorking: Bool
  var isCloning: Bool
  var isOpeningConsole: Bool
  var qemuLaunchPlan: QemuLaunchPlan?
  var onPrimaryAction: () async -> Void
  var onClone: () -> Void
  var onOpenConsole: () async -> Bool
  var onStop: () async -> Void
  var onRestart: () async -> Void

  @State private var isStopConfirmationPresented = false
  @State private var isRestartConfirmationPresented = false

  var body: some View {
    HStack(spacing: 12) {
      VStack(alignment: .leading, spacing: 4) {
        HStack(spacing: 8) {
          StatusDot(status: virtualMachine.status)
          Text(virtualMachine.name)
            .font(.title2.weight(.semibold))
        }

        Text("\(virtualMachine.status.title) - \(virtualMachine.mode.title)")
          .font(.subheadline)
          .foregroundStyle(.secondary)
      }

      Spacer()

      Button {
        onClone()
      } label: {
        if isCloning {
          ProgressView()
            .controlSize(.small)
        } else {
          Label("Clone", systemImage: "square.on.square")
        }
      }
      .disabled(isCloning)

      Button {
        Task { _ = await onOpenConsole() }
      } label: {
        if isOpeningConsole {
          ProgressView()
            .controlSize(.small)
        } else {
          Label(consoleCapability.actionTitle, systemImage: consoleCapability.actionSystemImage)
        }
      }
      .disabled(isOpeningConsole || !virtualMachine.canOpenConsole)

      Button {
        isRestartConfirmationPresented = true
      } label: {
        Label("Restart", systemImage: "arrow.clockwise")
      }
      .disabled(isWorking || virtualMachine.status == .stopped)

      Button(role: .destructive) {
        isStopConfirmationPresented = true
      } label: {
        Label("Stop", systemImage: "stop.fill")
      }
      .disabled(isWorking || virtualMachine.status == .stopped)

      Button {
        Task { await onPrimaryAction() }
      } label: {
        if isWorking {
          ProgressView()
            .controlSize(.small)
        } else {
          Label(virtualMachine.primaryActionTitle, systemImage: primaryActionIcon)
        }
      }
      .buttonStyle(.borderedProminent)
      .disabled(isWorking)
    }
    .padding(.horizontal, 24)
    .padding(.vertical, 16)
    .background(Color(nsColor: .windowBackgroundColor))
    .alert(
      VirtualMachineAction.restart.interruptingConfirmationTitle,
      isPresented: $isRestartConfirmationPresented
    ) {
      Button("Cancel", role: .cancel) {}
      Button(VirtualMachineAction.restart.interruptingConfirmationButtonTitle, role: .destructive) {
        Task { await onRestart() }
      }
    } message: {
      Text(
        VirtualMachineAction.restart.interruptingConfirmationMessage(vmName: virtualMachine.name))
    }
    .alert(
      VirtualMachineAction.stop.interruptingConfirmationTitle,
      isPresented: $isStopConfirmationPresented
    ) {
      Button("Cancel", role: .cancel) {}
      Button(VirtualMachineAction.stop.interruptingConfirmationButtonTitle, role: .destructive) {
        Task { await onStop() }
      }
    } message: {
      Text(VirtualMachineAction.stop.interruptingConfirmationMessage(vmName: virtualMachine.name))
    }
  }

  private var primaryActionIcon: String {
    switch virtualMachine.status {
    case .running: return "pause.fill"
    case .paused, .suspended: return "play.fill"
    case .stopped, .error: return "play.fill"
    }
  }

  private var consoleCapability: ConsoleCapability {
    ConsoleCapability.evaluate(for: virtualMachine, qemuLaunchPlan: qemuLaunchPlan)
  }
}

extension VirtualMachineAction {
  fileprivate var requiresInterruptingConfirmation: Bool {
    switch self {
    case .stop, .restart:
      return true
    case .start, .pause, .resume:
      return false
    }
  }

  fileprivate var interruptingConfirmationTitle: String {
    switch self {
    case .restart:
      return "Restart virtual machine?"
    case .stop:
      return "Stop virtual machine?"
    case .start, .pause, .resume:
      return ""
    }
  }

  fileprivate var interruptingConfirmationButtonTitle: String {
    switch self {
    case .restart:
      return "Restart"
    case .stop:
      return "Stop"
    case .start, .pause, .resume:
      return ""
    }
  }

  fileprivate func interruptingConfirmationMessage(vmName: String) -> String {
    switch self {
    case .restart:
      return "Restart \(vmName) now. Running work in the guest may be interrupted."
    case .stop:
      return "Stop \(vmName) now. Running work in the guest may be interrupted."
    case .start, .pause, .resume:
      return ""
    }
  }
}

private struct ConsoleDiagnosticsPanel: View {
  var virtualMachine: VirtualMachine
  var qmpStatus: QMPStatus?
  var qmpStatusError: String?
  var qemuLaunchPlan: QemuLaunchPlan?
  var qemuLaunchPlanError: String?
  var qemuLog: VMLogView?
  var serialLog: VMLogView?
  var isOpeningConsole: Bool
  var isLoadingLog: Bool
  var logViewError: String?
  var onOpenConsole: () async -> Bool
  var onLoadLog: (VMLogKind) async -> Void
  var onShowDisplay: (String, String) async -> Void = { _, _ in }
  @State private var displayWindowWidth = "1280"
  @State private var displayWindowHeight = "800"

  var body: some View {
    VStack(alignment: .leading, spacing: 14) {
      HStack(alignment: .top, spacing: 12) {
        VStack(alignment: .leading, spacing: 6) {
          Label("Metadata Diagnostics", systemImage: "stethoscope")
            .font(.headline)
          Text(virtualMachine.name)
            .font(.title3.weight(.semibold))
          Text(consoleDiagnosticsDetail)
          .font(.callout)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
        }

        Spacer()

        Button {
          Task { _ = await onOpenConsole() }
        } label: {
          if isOpeningConsole {
            ProgressView()
              .controlSize(.small)
          } else {
            Label(capability.actionTitle, systemImage: capability.actionSystemImage)
          }
        }
        .buttonStyle(.borderedProminent)
        .disabled(isOpeningConsole || !virtualMachine.canOpenConsole)
      }

      LazyVGrid(columns: [GridItem(.adaptive(minimum: 190), spacing: 12)], spacing: 12) {
        DiagnosticBadge(
          title: "Viewer",
          value: capability.graphicalViewerTitle,
          systemImage: capability.graphicalViewerAvailable ? "display" : "display.trianglebadge.exclamationmark"
        )
        DiagnosticBadge(
          title: "QMP Socket Probe",
          value: qmpSocketProbeTitle,
          systemImage: capability.qmpDiagnosticsAvailable
            ? "point.3.connected.trianglepath.dotted" : "lock"
        )
        DiagnosticBadge(
          title: "Log Tails",
          value: capability.boundedLogTailsTitle,
          systemImage: capability.boundedLogTailsAvailable ? "doc.text" : "doc.badge.plus"
        )
        DiagnosticBadge(
          title: "VM State",
          value: virtualMachine.status.title,
          systemImage: statusSystemImage
        )
        DiagnosticBadge(
          title: "QMP",
          value: qmpSummary,
          systemImage: qmpStatus?.available == true ? "checkmark.circle" : "xmark.circle"
        )
        DiagnosticBadge(
          title: "QEMU Tail",
          value: logSummary(qemuLog),
          systemImage: qemuLog?.exists == true ? "doc.text" : "doc.badge.plus"
        )
        DiagnosticBadge(
          title: "Serial Tail",
          value: logSummary(serialLog),
          systemImage: serialLog?.exists == true ? "text.alignleft" : "doc.badge.plus"
        )
      }

      VStack(alignment: .leading, spacing: 8) {
        DiagnosticFactRow(title: "Guest", value: virtualMachine.guest)
        DiagnosticFactRow(title: "Mode", value: virtualMachine.mode.title)
        DiagnosticFactRow(title: "Resources", value: resourcesText)
        DiagnosticFactRow(title: "Network", value: virtualMachine.ipAddress ?? "No inventory IP")
        DiagnosticFactRow(title: "Uptime", value: virtualMachine.uptime)
        if let viewerEndpoint = qemuLaunchPlan?.viewerEndpoint {
          DiagnosticFactRow(title: "Viewer Endpoint", value: viewerEndpoint.absoluteString)
        }
        if let qmpStatus {
          DiagnosticFactRow(title: "QMP Socket", value: qmpStatus.socketPath)
          DiagnosticFactRow(title: "QMP Socket Status", value: qmpStatus.readinessTitle)
          if let supervisor = qmpStatus.supervisor {
            DiagnosticFactRow(title: "QMP Supervisor", value: supervisor.summaryTitle)
            DiagnosticFactRow(
              title: "QMP Envelopes",
              value: "\(supervisor.envelopesRead)"
            )
          }
        }
        if let latestLogPath {
          DiagnosticFactRow(title: "Latest Log", value: latestLogPath)
        }
      }

      if let qmpStatusError {
        ErrorLabel(message: qmpStatusError)
      }

      if let qemuLaunchPlanError {
        ErrorLabel(message: qemuLaunchPlanError)
      }

      if let logViewError {
        ErrorLabel(message: logViewError)
      }

      HStack(spacing: 10) {
        Button {
          Task { await onLoadLog(.qemu) }
        } label: {
          if isLoadingLog {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Refresh QEMU Tail", systemImage: "terminal")
          }
        }
        .disabled(isLoadingLog)

        Button {
          Task { await onLoadLog(.serial) }
        } label: {
          Label("Refresh Serial Tail", systemImage: "text.alignleft")
        }
        .disabled(isLoadingLog)

        if virtualMachine.mode == .fast {
          // Fast Mode (Apple VZ) only: open the in-app VZVirtualMachineView
          // window via the bundled runner (must be a GUI login session).
          TextField("Width", text: $displayWindowWidth)
            .textFieldStyle(.roundedBorder)
            .frame(width: 72)
          TextField("Height", text: $displayWindowHeight)
            .textFieldStyle(.roundedBorder)
            .frame(width: 72)
          Button {
            Task { await onShowDisplay(displayWindowWidth, displayWindowHeight) }
          } label: {
            Label("Show Display", systemImage: "display")
          }
          .help("Open this Fast Mode VM in an embedded display window")
        }
      }
      .controlSize(.small)
    }
    .padding(14)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private var qmpSummary: String {
    guard let qmpStatus else {
      return virtualMachine.canOpenConsole ? "Socket not probed" : "Socket unavailable"
    }

    return qmpStatus.available ? qmpStatus.readinessTitle : "Socket unavailable"
  }

  private var qmpSocketProbeTitle: String {
    capability.qmpDiagnosticsAvailable ? "Socket probe available" : "Requires running VM"
  }

  private var consoleDiagnosticsDetail: String {
    if capability.graphicalViewerAvailable {
      return "Graphical console path is advertised; verify viewer output separately from QMP socket diagnostics and bounded logs."
    }

    if capability.qmpDiagnosticsAvailable {
      return "QMP probing checks diagnostic socket availability only; bounded log tails remain metadata-safe."
    }

    return "QMP socket diagnostics require a running VM; bounded log tails remain metadata-safe."
  }

  private var capability: ConsoleCapability {
    ConsoleCapability.evaluate(for: virtualMachine, qemuLaunchPlan: qemuLaunchPlan)
  }

  private var resourcesText: String {
    "\(virtualMachine.resources.cpuCount) CPU / \(virtualMachine.resources.memoryGB) GB / \(virtualMachine.resources.diskGB) GB"
  }

  private var latestLogPath: String? {
    qemuLog?.path ?? serialLog?.path
  }

  private func logSummary(_ log: VMLogView?) -> String {
    guard let log else {
      return "Not loaded"
    }

    guard log.exists else {
      return "Missing"
    }

    let suffix = log.truncated ? " tail" : ""
    return "\(log.returnedBytes)/\(log.bytes) bytes\(suffix)"
  }

  private var statusSystemImage: String {
    switch virtualMachine.status {
    case .running:
      return "play.circle"
    case .paused:
      return "pause.circle"
    case .stopped:
      return "stop.circle"
    case .suspended:
      return "power.circle"
    case .error:
      return "exclamationmark.triangle"
    }
  }
}

private struct DiagnosticBadge: View {
  var title: String
  var value: String
  var systemImage: String

  var body: some View {
    HStack(alignment: .top, spacing: 10) {
      Image(systemName: systemImage)
        .font(.title3)
        .foregroundStyle(.secondary)
        .frame(width: 24)

      VStack(alignment: .leading, spacing: 4) {
        Text(title)
          .font(.caption)
          .foregroundStyle(.secondary)
        Text(value)
          .font(.callout.weight(.medium))
          .lineLimit(2)
          .minimumScaleFactor(0.85)
      }
    }
    .frame(maxWidth: .infinity, minHeight: 58, alignment: .leading)
    .padding(10)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }
}

private struct DiagnosticFactRow: View {
  var title: String
  var value: String

  var body: some View {
    HStack(alignment: .firstTextBaseline, spacing: 12) {
      Text(title)
        .font(.caption)
        .foregroundStyle(.secondary)
        .frame(width: 96, alignment: .leading)

      Text(value)
        .font(.caption.monospaced())
        .lineLimit(2)
        .truncationMode(.middle)
        .textSelection(.enabled)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
  }
}

private struct VMReadinessNextActionPanel: View {
  var virtualMachine: VirtualMachine
  var readinessReport: VMReadinessReport?
  var isLoadingReadinessReport: Bool
  var readinessReportError: String?
  var bootMediaStatus: BootMediaStatus?
  var isLoadingBootMediaStatus: Bool
  var bootMediaStatusError: String?
  var guestToolsStatus: GuestToolsStatus?
  var isLoadingGuestToolsStatus: Bool
  var guestToolsStatusError: String?
  var runnerStatus: RunnerStatus?
  var isLoadingRunnerStatus: Bool
  var runnerStatusError: String?
  var snapshotPreflightStatus: SnapshotPreflightStatus?
  var isLoadingSnapshotPreflightStatus: Bool
  var snapshotPreflightStatusError: String?
  var snapshotChain: VMSnapshotChain?
  var snapshotChainError: String?
  var diskPreparation: DiskPreparation?
  var diskCreation: VMDiskCreation?
  var diskInspection: VMDiskInspection?
  var diskVerification: VMDiskVerification?
  var qmpStatus: QMPStatus?
  var qmpStatusError: String?
  var isWorking: Bool
  var isOpeningConsole: Bool
  var onPrimaryAction: () async -> Void
  var onOpenConsole: () async -> Bool
  var onRefreshBootMediaStatus: () async -> Void
  var onRefreshGuestToolsStatus: () async -> Void
  var onPrepareRun: () async -> Bool
  var onPreparePrimaryDisk: () async -> Bool
  var onRefreshRunnerStatus: () async -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack(alignment: .top, spacing: 12) {
        VStack(alignment: .leading, spacing: 4) {
          Label("Readiness", systemImage: "checklist.checked")
            .font(.headline)
          Text(nextAction.detail)
            .font(.callout)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        }

        Spacer()

        nextActionButton
          .controlSize(.small)
      }

      LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 10)], spacing: 10) {
        ForEach(readinessItems) { item in
          ReadinessSummaryItem(item: item)
        }
      }

      if let aggregateStatus {
        Divider()

        Label(aggregateStatus, systemImage: aggregateSystemImage)
          .font(.caption)
          .foregroundStyle(aggregateTint)
          .fixedSize(horizontal: false, vertical: true)
      }

      if let liveEvidence = readinessReport?.liveEvidence {
        LiveEvidenceRow(evidence: liveEvidence)
      }

      if let readinessReport, !readinessReport.evidenceRequirements.isEmpty {
        VStack(alignment: .leading, spacing: 8) {
          Text("Evidence Requirements")
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)

          ForEach(readinessReport.evidenceRequirements) { requirement in
            EvidenceRequirementRow(requirement: requirement)
          }
        }
      }
    }
    .padding(12)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  @ViewBuilder
  private var nextActionButton: some View {
    switch nextAction.kind {
    case .wait:
      Label(nextAction.title, systemImage: nextAction.systemImage)
        .foregroundStyle(.secondary)
    case .refreshBootMedia:
      Button {
        Task { await onRefreshBootMediaStatus() }
      } label: {
        Label(nextAction.title, systemImage: nextAction.systemImage)
      }
      .disabled(isLoadingBootMediaStatus)
    case .prepareRun:
      Button {
        Task { _ = await onPrepareRun() }
      } label: {
        Label(nextAction.title, systemImage: nextAction.systemImage)
      }
      .buttonStyle(.borderedProminent)
      .disabled(isLoadingRunnerStatus || isWorking)
    case .prepareDisk:
      Button {
        Task { _ = await onPreparePrimaryDisk() }
      } label: {
        Label(nextAction.title, systemImage: nextAction.systemImage)
      }
      .buttonStyle(.borderedProminent)
      .disabled(isWorking)
    case .refreshRunner:
      Button {
        Task { await onRefreshRunnerStatus() }
      } label: {
        Label(nextAction.title, systemImage: nextAction.systemImage)
      }
      .disabled(isLoadingRunnerStatus)
    case .primary:
      Button {
        Task { await onPrimaryAction() }
      } label: {
        if isWorking {
          ProgressView()
            .controlSize(.small)
        } else {
          Label(nextAction.title, systemImage: nextAction.systemImage)
        }
      }
      .buttonStyle(.borderedProminent)
      .disabled(isWorking)
    case .openConsole:
      Button {
        Task { _ = await onOpenConsole() }
      } label: {
        if isOpeningConsole {
          ProgressView()
            .controlSize(.small)
        } else {
          Label(nextAction.title, systemImage: nextAction.systemImage)
        }
      }
      .buttonStyle(.borderedProminent)
      .disabled(isOpeningConsole || !virtualMachine.canOpenConsole)
    case .refreshGuestTools:
      Button {
        Task { await onRefreshGuestToolsStatus() }
      } label: {
        Label(nextAction.title, systemImage: nextAction.systemImage)
      }
      .disabled(isLoadingGuestToolsStatus)
    }
  }

  private var readinessItems: [ReadinessSummary] {
    [
      ReadinessSummary(
        title: "VM",
        value: virtualMachine.status.title,
        systemImage: statusSystemImage,
        tint: virtualMachine.status.tint
      ),
      ReadinessSummary(
        title: "Boot Media",
        value: bootMediaReadinessTitle,
        systemImage: bootMediaSystemImage,
        tint: bootMediaTint
      ),
      ReadinessSummary(
        title: "Launch Plan",
        value: launchReadinessTitle,
        systemImage: launchSystemImage,
        tint: launchTint
      ),
      ReadinessSummary(
        title: "Guest Tools",
        value: guestToolsReadinessTitle,
        systemImage: guestToolsSystemImage,
        tint: guestToolsTint
      ),
      ReadinessSummary(
        title: "Snapshot",
        value: snapshotReadinessTitle,
        systemImage: snapshotSystemImage,
        tint: snapshotTint
      ),
      ReadinessSummary(
        title: "QMP",
        value: qmpReadinessTitle,
        systemImage: qmpSystemImage,
        tint: qmpTint
      ),
    ]
  }

  private var nextAction: ReadinessNextAction {
    if readinessIsLoading {
      return ReadinessNextAction(
        kind: .wait,
        title: "Checking",
        detail: "Metadata checks are still updating.",
        systemImage: "hourglass"
      )
    }

    let summary = VMReadinessSummary.evaluate(
      virtualMachine: virtualMachine,
      bootMediaStatus: bootMediaStatus,
      bootMediaStatusError: bootMediaStatusError,
      runnerStatus: runnerStatus,
      runnerStatusError: runnerStatusError,
      preRunLaunchReadiness: runnerStatus == nil ? readinessReport?.preRunLaunchReadiness : nil,
      snapshotChain: snapshotChain,
      snapshotChainError: snapshotChainError,
      diskPreparation: diskPreparation,
      diskCreation: diskCreation,
      diskInspection: diskInspection,
      diskVerification: diskVerification
    )
    return ReadinessNextAction(summary: summary)
  }

  private var readinessIsLoading: Bool {
    isLoadingReadinessReport || isLoadingBootMediaStatus || isLoadingGuestToolsStatus
      || isLoadingRunnerStatus
      || isLoadingSnapshotPreflightStatus
  }

  private var aggregateStatus: String? {
    if isLoadingReadinessReport {
      return "Loading aggregate readiness report"
    }

    if let readinessReportError {
      return "Readiness report failed: \(readinessReportError)"
    }

    guard let readinessReport else {
      return nil
    }

    if readinessReport.blockers.isEmpty {
      if readinessReport.liveE2ERequired || !readinessReport.pendingRequiredEvidence.isEmpty {
        let pending = readinessReport.pendingRequiredEvidence.map(\.title).joined(separator: ", ")
        if pending.isEmpty {
          if readinessReport.liveEvidenceVerifiedForDisplay {
            return "Metadata checks clear; live evidence verified"
          }

          return "Metadata checks clear; live E2E evidence still required"
        }

        return "Metadata checks clear; \(readinessReport.liveEvidenceReadinessTitle.lowercased()): \(pending)"
      }

      return readinessReport.notes.first ?? "Aggregate readiness report loaded"
    }

    return readinessReport.blockers.prefix(2).joined(separator: " | ")
  }

  private var aggregateSystemImage: String {
    if isLoadingReadinessReport {
      return "arrow.triangle.2.circlepath"
    }

    if readinessReportError != nil {
      return "exclamationmark.triangle"
    }

    guard let readinessReport else {
      return "checkmark.seal"
    }

    guard readinessReport.blockers.isEmpty else {
      return "exclamationmark.octagon"
    }

    if !readinessReport.pendingRequiredEvidence.isEmpty
      || (readinessReport.liveE2ERequired && readinessReport.liveEvidence == nil)
    {
      return "clock.badge.exclamationmark"
    }

    return "checkmark.seal"
  }

  private var aggregateTint: Color {
    if isLoadingReadinessReport {
      return .secondary
    }

    if readinessReportError != nil {
      return .orange
    }

    guard let readinessReport else {
      return .green
    }

    guard readinessReport.blockers.isEmpty else {
      return .red
    }

    if !readinessReport.pendingRequiredEvidence.isEmpty
      || (readinessReport.liveE2ERequired && readinessReport.liveEvidence == nil)
    {
      return .orange
    }

    return .green
  }

  private var missingBootMediaCount: Int {
    bootMediaStatus?.entries.filter { !$0.exists }.count ?? 0
  }

  private var bootMediaReadinessTitle: String {
    if isLoadingBootMediaStatus {
      return "Checking"
    }

    if bootMediaStatusError != nil {
      return "Needs review"
    }

    guard let bootMediaStatus else {
      return "Not checked"
    }

    if bootMediaStatus.entries.isEmpty {
      return "No entries"
    }

    return missingBootMediaCount == 0 ? "Present" : "Missing \(missingBootMediaCount)"
  }

  private var bootMediaSystemImage: String {
    if bootMediaStatusError != nil || missingBootMediaCount > 0 {
      return "externaldrive.badge.exclamationmark"
    }
    return bootMediaStatus == nil ? "externaldrive.badge.questionmark" : "externaldrive"
  }

  private var bootMediaTint: Color {
    if bootMediaStatusError != nil || missingBootMediaCount > 0 {
      return .orange
    }
    return bootMediaStatus == nil ? .secondary : .green
  }

  private var launchReadinessTitle: String {
    if isLoadingRunnerStatus {
      return "Checking"
    }

    if runnerStatusError != nil {
      return "Needs review"
    }

    if let runnerStatus {
      return runnerStatus.launchReadinessTitle
    }

    return readinessReport?.preRunLaunchReadiness?.title ?? "Not prepared"
  }

  private var launchSystemImage: String {
    if runnerStatusError != nil {
      return "exclamationmark.triangle"
    }

    return launchReadinessIsReady
      ? "checkmark.circle" : "list.bullet.clipboard"
  }

  private var launchTint: Color {
    if runnerStatusError != nil {
      return .orange
    }

    return launchReadinessIsReady ? .green : .secondary
  }

  private var launchReadinessIsReady: Bool {
    if let runnerStatus {
      return runnerStatus.launchReadiness?.ready == true
    }

    return readinessReport?.preRunLaunchReadiness?.ready == true
  }

  private var guestToolsReadinessTitle: String {
    if isLoadingGuestToolsStatus {
      return "Checking"
    }

    if guestToolsStatusError != nil {
      return "Needs review"
    }

    return guestToolsStatus?.networkReadinessTitle ?? "Not checked"
  }

  private var guestToolsSystemImage: String {
    if guestToolsStatusError != nil {
      return "exclamationmark.triangle"
    }

    return guestToolsStatus?.connected == true ? "link" : "link.badge.plus"
  }

  private var guestToolsTint: Color {
    if guestToolsStatusError != nil {
      return .orange
    }

    return guestToolsStatus?.connected == true ? .green : .secondary
  }

  private var snapshotReadinessTitle: String {
    if isLoadingSnapshotPreflightStatus {
      return "Checking"
    }

    if snapshotPreflightStatusError != nil {
      return "Needs review"
    }

    return snapshotPreflightStatus?.readinessTitle ?? "Not checked"
  }

  private var snapshotSystemImage: String {
    if snapshotPreflightStatusError != nil {
      return "exclamationmark.triangle"
    }

    return snapshotPreflightStatus?.ready == true
      ? "camera.badge.checkmark" : "camera.metering.matrix"
  }

  private var snapshotTint: Color {
    if snapshotPreflightStatusError != nil {
      return .orange
    }

    return snapshotPreflightStatus?.ready == true ? .green : .secondary
  }

  private var qmpReadinessTitle: String {
    if qmpStatusError != nil {
      return "Needs review"
    }

    return qmpStatus?.readinessTitle ?? "Socket not probed"
  }

  private var qmpSystemImage: String {
    if qmpStatusError != nil {
      return "exclamationmark.triangle"
    }

    return qmpStatus?.available == true ? "point.3.connected.trianglepath.dotted" : "xmark.circle"
  }

  private var qmpTint: Color {
    if qmpStatusError != nil {
      return .orange
    }

    return qmpStatus?.available == true ? .green : .secondary
  }

  private var statusSystemImage: String {
    switch virtualMachine.status {
    case .running:
      return "play.circle"
    case .paused:
      return "pause.circle"
    case .stopped:
      return "stop.circle"
    case .suspended:
      return "power.circle"
    case .error:
      return "exclamationmark.triangle"
    }
  }

  private var primaryActionIcon: String {
    switch virtualMachine.status {
    case .running:
      return "pause.fill"
    case .paused, .suspended, .stopped, .error:
      return "play.fill"
    }
  }
}

private struct ReadinessSummary: Identifiable {
  var title: String
  var value: String
  var systemImage: String
  var tint: Color

  var id: String { title }
}

private struct ReadinessSummaryItem: View {
  var item: ReadinessSummary

  var body: some View {
    HStack(alignment: .top, spacing: 8) {
      Image(systemName: item.systemImage)
        .foregroundStyle(item.tint)
        .frame(width: 18)

      VStack(alignment: .leading, spacing: 2) {
        Text(item.title)
          .font(.caption)
          .foregroundStyle(.secondary)
        Text(item.value)
          .font(.callout.weight(.medium))
          .lineLimit(2)
          .minimumScaleFactor(0.85)
      }
    }
    .frame(maxWidth: .infinity, minHeight: 48, alignment: .leading)
    .padding(8)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
  }
}

private struct LiveEvidenceRow: View {
  var evidence: VMLiveEvidence

  private var consoleEvidenceProven: Bool {
    evidence.interactiveConsoleEvidenceProven
  }

  private var proofComplete: Bool {
    consoleEvidenceProven && evidence.guestToolsEffectsProven
  }

  var body: some View {
    HStack(alignment: .top, spacing: 8) {
      Image(systemName: proofComplete ? "checkmark.seal" : "exclamationmark.circle")
        .foregroundStyle(proofComplete ? .green : .orange)
        .frame(width: 18)

      VStack(alignment: .leading, spacing: 3) {
        Text(evidence.title)
          .font(.caption.weight(.medium))
        Text(evidence.detail)
          .font(.caption)
          .foregroundStyle(.secondary)
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 128), spacing: 6)], spacing: 6) {
          ForEach(evidence.proofItems) { item in
            LiveEvidenceProofItemView(item: item)
          }
        }
        .padding(.top, 2)
        Text(evidence.path)
          .font(.caption2)
          .foregroundStyle(.secondary)
          .lineLimit(1)
          .truncationMode(.middle)
      }
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(8)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
  }
}

private struct LiveEvidenceProofItemView: View {
  var item: VMLiveEvidenceProofItem

  var body: some View {
    HStack(spacing: 5) {
      Image(systemName: item.proven ? "checkmark.circle.fill" : "clock")
        .foregroundStyle(item.proven ? .green : .orange)
        .font(.caption2)

      VStack(alignment: .leading, spacing: 1) {
        Text(item.title)
          .font(.caption2.weight(.medium))
        Text(item.status)
          .font(.caption2)
          .foregroundStyle(.secondary)
      }
    }
    .frame(maxWidth: .infinity, minHeight: 32, alignment: .leading)
    .padding(.horizontal, 6)
    .padding(.vertical, 5)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
    .help(item.detail)
  }
}

private struct EvidenceRequirementRow: View {
  var requirement: VMEvidenceRequirement

  var body: some View {
    HStack(alignment: .top, spacing: 8) {
      Image(systemName: requirement.proven ? "checkmark.circle" : "exclamationmark.circle")
        .foregroundStyle(tint)
        .frame(width: 18)

      VStack(alignment: .leading, spacing: 3) {
        HStack(spacing: 6) {
          Text(requirement.title)
            .font(.caption.weight(.medium))
          Text(requirement.required ? "required" : "optional")
            .font(.caption2)
            .foregroundStyle(.secondary)
          Text(requirement.proven ? "proven" : "pending")
            .font(.caption2.weight(.medium))
            .foregroundStyle(tint)
        }

        Text(requirement.note)
          .font(.caption)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      }
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(8)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
  }

  private var tint: Color {
    requirement.proven ? .green : (requirement.required ? .orange : .secondary)
  }
}

private struct ReadinessNextAction {
  enum Kind {
    case wait
    case refreshBootMedia
    case prepareDisk
    case prepareRun
    case refreshRunner
    case primary
    case openConsole
    case refreshGuestTools
  }

  var kind: Kind
  var title: String
  var detail: String
  var systemImage: String

  init(kind: Kind, title: String, detail: String, systemImage: String) {
    self.kind = kind
    self.title = title
    self.detail = detail
    self.systemImage = systemImage
  }

  init(summary: VMReadinessSummary) {
    title = summary.actionTitle
    detail = Self.detail(for: summary)
    systemImage = Self.systemImage(for: summary.action)
    kind = Self.kind(for: summary.action)
  }

  private static func detail(for summary: VMReadinessSummary) -> String {
    switch summary.action {
    case .openConsole:
      return "Probe QMP socket availability and refresh bounded log tails; this does not confirm guest command effects."
    case .refreshBootMedia, .prepareDisk, .prepareRun, .refreshRunner, .primaryAction:
      return summary.detail
    }
  }

  private static func kind(for action: VMReadinessSummary.Action) -> Kind {
    switch action {
    case .refreshBootMedia:
      return .refreshBootMedia
    case .prepareDisk:
      return .prepareDisk
    case .prepareRun:
      return .prepareRun
    case .refreshRunner:
      return .refreshRunner
    case .openConsole:
      return .openConsole
    case .primaryAction:
      return .primary
    }
  }

  private static func systemImage(for action: VMReadinessSummary.Action) -> String {
    switch action {
    case .refreshBootMedia:
      return "externaldrive.badge.questionmark"
    case .prepareDisk:
      return "internaldrive"
    case .prepareRun:
      return "list.bullet.clipboard"
    case .refreshRunner:
      return "arrow.clockwise"
    case .openConsole:
      return "point.3.connected.trianglepath.dotted"
    case .primaryAction:
      return "play.fill"
    }
  }
}

private struct LifecycleControlsPanel: View {
  var virtualMachine: VirtualMachine
  var actions: [LifecycleActionOption]
  var isWorking: Bool
  var plan: LifecyclePlan?
  var isLoadingPlan: Bool
  var planError: String?
  var onInspectPlan: (LifecyclePlanAction) async -> Void
  var onPerform: (VirtualMachineAction) async -> Void

  @State private var pendingInterruptingAction: VirtualMachineAction?
  @State private var isInterruptingConfirmationPresented = false

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Text("Lifecycle")
          .font(.headline)

        Spacer()

        Label(virtualMachine.status.title, systemImage: statusSystemImage)
          .font(.callout)
          .foregroundStyle(.secondary)
      }

      LazyVGrid(columns: [GridItem(.adaptive(minimum: 190), spacing: 12)], spacing: 12) {
        ForEach(actions) { option in
          Button {
            if option.action.requiresInterruptingConfirmation {
              pendingInterruptingAction = option.action
              isInterruptingConfirmationPresented = true
            } else {
              Task { await onPerform(option.action) }
            }
          } label: {
            HStack(alignment: .top, spacing: 10) {
              if isWorking {
                ProgressView()
                  .controlSize(.small)
                  .frame(width: 24)
              } else {
                Image(systemName: option.systemImage)
                  .font(.title3)
                  .foregroundStyle(option.isDestructive ? .red : .secondary)
                  .frame(width: 24)
              }

              VStack(alignment: .leading, spacing: 4) {
                Text(option.title)
                  .font(.callout.weight(.medium))
                Text(option.detail)
                  .font(.caption)
                  .foregroundStyle(.secondary)
                  .fixedSize(horizontal: false, vertical: true)
              }

              Spacer(minLength: 0)
            }
            .frame(maxWidth: .infinity, minHeight: 64, alignment: .leading)
            .contentShape(Rectangle())
          }
          .buttonStyle(.bordered)
          .disabled(isWorking)
        }
      }

      Divider()

      VStack(alignment: .leading, spacing: 10) {
        HStack(spacing: 8) {
          Text("Command Readiness")
            .font(.callout)
            .foregroundStyle(.secondary)

          Spacer()

          Button {
            Task { await onInspectPlan(.suspend) }
          } label: {
            planButtonLabel("Plan Suspend", systemImage: "pause.circle")
          }
          .controlSize(.small)
          .disabled(isLoadingPlan)

          Button {
            Task { await onInspectPlan(.resume) }
          } label: {
            planButtonLabel("Plan Resume", systemImage: "play.circle")
          }
          .controlSize(.small)
          .disabled(isLoadingPlan)
        }

        if let planError {
          Label(planError, systemImage: "exclamationmark.triangle")
            .font(.callout)
            .foregroundStyle(.red)
        } else if let plan {
          LifecyclePlanSummary(plan: plan)
        } else {
          Text("Inspect suspend or resume readiness before sending backend lifecycle control.")
            .font(.callout)
            .foregroundStyle(.secondary)
        }
      }
    }
    .padding(12)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
    .alert(
      pendingInterruptingAction?.interruptingConfirmationTitle ?? "Confirm lifecycle action?",
      isPresented: $isInterruptingConfirmationPresented
    ) {
      Button("Cancel", role: .cancel) {
        pendingInterruptingAction = nil
      }
      Button(
        pendingInterruptingAction?.interruptingConfirmationButtonTitle ?? "Continue",
        role: .destructive
      ) {
        guard let action = pendingInterruptingAction else {
          return
        }
        pendingInterruptingAction = nil
        Task { await onPerform(action) }
      }
    } message: {
      Text(
        pendingInterruptingAction?.interruptingConfirmationMessage(vmName: virtualMachine.name)
          ?? "This lifecycle action may interrupt running work in the guest."
      )
    }
  }

  @ViewBuilder
  private func planButtonLabel(_ title: String, systemImage: String) -> some View {
    if isLoadingPlan {
      ProgressView()
        .controlSize(.small)
    } else {
      Label(title, systemImage: systemImage)
    }
  }

  private var statusSystemImage: String {
    switch virtualMachine.status {
    case .running:
      return "play.circle"
    case .paused:
      return "pause.circle"
    case .stopped:
      return "stop.circle"
    case .suspended:
      return "power.circle"
    case .error:
      return "exclamationmark.triangle"
    }
  }
}

private struct MetricCard: View {
  var title: String
  var value: String
  var systemImage: String

  var body: some View {
    HStack(alignment: .top, spacing: 12) {
      Image(systemName: systemImage)
        .font(.title3)
        .foregroundStyle(.secondary)
        .frame(width: 28)

      VStack(alignment: .leading, spacing: 5) {
        Text(title)
          .font(.caption)
          .foregroundStyle(.secondary)
        Text(value)
          .font(.headline)
          .lineLimit(2)
          .minimumScaleFactor(0.85)
      }
    }
    .frame(maxWidth: .infinity, minHeight: 76, alignment: .leading)
    .padding(14)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }
}

private struct TimelineRow: View {
  var title: String
  var detail: String

  var body: some View {
    HStack(alignment: .firstTextBaseline) {
      Text(title)
        .foregroundStyle(.secondary)
        .frame(width: 110, alignment: .leading)

      Text(detail)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
    .font(.body)
    .padding(.vertical, 4)
  }
}

private struct LifecyclePlanSummary: View {
  var plan: LifecyclePlan

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 10)], spacing: 10) {
        GuestToolsStatusBadge(
          title: "Action",
          value: plan.action.title,
          systemImage: plan.action == .suspend ? "pause.circle" : "play.circle"
        )
        GuestToolsStatusBadge(
          title: "Backend",
          value: plan.backend,
          systemImage: plan.backend == "qemu-qmp" ? "terminal" : "desktopcomputer"
        )
        GuestToolsStatusBadge(
          title: "Plan",
          value: plan.executable ? "Ready" : "Blocked",
          systemImage: plan.executable ? "checkmark.circle" : "exclamationmark.triangle"
        )
        GuestToolsStatusBadge(
          title: "QMP Socket",
          value: plan.socketAvailable ? "Available" : "Unavailable",
          systemImage: plan.socketAvailable
            ? "point.3.connected.trianglepath.dotted" : "xmark.circle"
        )
      }

      GuestToolsFactRow(title: "Current", value: plan.currentState.title)
      GuestToolsFactRow(title: "Target", value: plan.targetState.title)
      GuestToolsFactRow(title: "Metadata only", value: plan.metadataOnly ? "true" : "false")
      if let qmpCommand = plan.qmpCommand {
        GuestToolsFactRow(title: "QMP command", value: qmpCommand)
      }
      if let socketPath = plan.socketPath {
        Text(socketPath)
          .font(.caption.monospaced())
          .foregroundStyle(.secondary)
          .lineLimit(2)
      }

      if plan.blockers.isEmpty {
        Label("No readiness blockers reported.", systemImage: "checkmark.circle")
          .font(.callout)
          .foregroundStyle(.secondary)
      } else {
        VStack(alignment: .leading, spacing: 6) {
          ForEach(plan.blockers, id: \.self) { blocker in
            Text(blocker)
              .font(.caption)
              .foregroundStyle(.secondary)
              .fixedSize(horizontal: false, vertical: true)
          }
        }
        .padding(8)
        .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
      }

      ForEach(plan.notes, id: \.self) { note in
        Text(note)
          .font(.caption)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      }
    }
  }
}

private func parsePort(_ value: String) -> UInt16? {
  guard let port = UInt16(value.trimmingCharacters(in: .whitespacesAndNewlines)), port > 0 else {
    return nil
  }
  return port
}

private struct OpenPortPlanPanel: View {
  var plan: OpenPortPlan?
  var isLoading: Bool
  var error: String?
  var onInspect: (String, String) async -> Bool

  @State private var guestPort = "80"
  @State private var scheme = "http"

  private var canInspect: Bool {
    parsePort(guestPort) != nil
      && !scheme.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Text("Open Port")
          .font(.headline)

        Spacer()

        Label("Metadata plan", systemImage: "network")
          .font(.callout)
          .foregroundStyle(.secondary)
      }

      HStack(spacing: 10) {
        TextField("Guest port", text: $guestPort)
          .textFieldStyle(.roundedBorder)
          .frame(width: 120)

        TextField("Scheme", text: $scheme)
          .textFieldStyle(.roundedBorder)
          .frame(width: 120)

        Button {
          Task { _ = await onInspect(guestPort, scheme) }
        } label: {
          if isLoading {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Plan Open", systemImage: "safari")
          }
        }
        .buttonStyle(.borderedProminent)
        .disabled(isLoading || !canInspect)
      }

      if let error {
        Label(error, systemImage: "exclamationmark.triangle")
          .font(.callout)
          .foregroundStyle(.red)
      } else if let plan {
        OpenPortPlanSummary(plan: plan)
      } else {
        Text("Plan a browser target for an existing host port forward without launching a browser.")
          .font(.callout)
          .foregroundStyle(.secondary)
      }
    }
    .padding(12)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }
}

private struct PortForwardManifestPanel: View {
  var list: VMPortForwardList?
  var isLoading: Bool
  var isAdding: Bool
  var isRemoving: Bool
  var error: String?
  var onRefresh: () async -> Void
  var onAdd: (String, String) async -> Bool
  var onRemove: (UInt16, UInt16) async -> Bool

  @State private var hostPort = "3000"
  @State private var guestPort = "3000"
  @State private var pendingPortForwardRemoval: VMPortForward?
  @State private var isRemoveConfirmationPresented = false

  private var canAddForward: Bool {
    parsePort(hostPort) != nil && parsePort(guestPort) != nil
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Text("Port Forwards")
          .font(.headline)

        Spacer()

        Label("Manifest policy", systemImage: "point.3.connected.trianglepath.dotted")
          .font(.callout)
          .foregroundStyle(.secondary)
      }

      Text(
        "Manage recorded host-to-guest port forwards without opening a browser, changing live networking, or starting a VM."
      )
      .font(.callout)
      .foregroundStyle(.secondary)

      HStack(spacing: 10) {
        Button {
          Task { await onRefresh() }
        } label: {
          if isLoading {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Refresh", systemImage: "arrow.clockwise")
          }
        }
        .buttonStyle(.bordered)
        .disabled(isLoading)

        TextField("Host port", text: $hostPort)
          .textFieldStyle(.roundedBorder)
          .frame(width: 110)

        TextField("Guest port", text: $guestPort)
          .textFieldStyle(.roundedBorder)
          .frame(width: 110)

        Button {
          Task { _ = await onAdd(hostPort, guestPort) }
        } label: {
          if isAdding {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Add", systemImage: "plus.circle")
          }
        }
        .buttonStyle(.borderedProminent)
        .disabled(isAdding || !canAddForward)
      }

      if let error {
        ErrorLabel(message: error)
      }

      if let list {
        if list.forwards.isEmpty {
          Label("No manifest port forwards recorded.", systemImage: "tray")
            .font(.callout)
            .foregroundStyle(.secondary)
        } else {
          VStack(alignment: .leading, spacing: 8) {
            ForEach(list.forwards) { forward in
              HStack(spacing: 10) {
                Label("\(forward.host) -> \(forward.guest)", systemImage: "arrow.right.circle")
                  .font(.callout.weight(.medium))
                Spacer()
                Button {
                  pendingPortForwardRemoval = forward
                  isRemoveConfirmationPresented = true
                } label: {
                  if isRemoving {
                    ProgressView()
                      .controlSize(.small)
                  } else {
                    Label("Remove", systemImage: "minus.circle")
                  }
                }
                .buttonStyle(.bordered)
                .disabled(isRemoving)
              }
              .padding(10)
              .background(
                Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
            }
          }
        }
      }
    }
    .padding(12)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
    .alert("Remove port forward?", isPresented: $isRemoveConfirmationPresented) {
      Button("Cancel", role: .cancel) {}
      Button("Remove", role: .destructive) {
        guard let forward = pendingPortForwardRemoval else {
          return
        }
        pendingPortForwardRemoval = nil
        Task { _ = await onRemove(forward.host, forward.guest) }
      }
    } message: {
      if let forward = pendingPortForwardRemoval {
        Text(
          "Remove host port \(forward.host) forwarding to guest port \(forward.guest) from the VM manifest."
        )
      }
    }
  }
}

private struct SSHPlanPanel: View {
  var plan: SSHPlan?
  var isLoading: Bool
  var error: String?
  var onInspect: (String) async -> Bool

  @State private var user = "user"

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Text("SSH Plan")
          .font(.headline)

        Spacer()

        Label("Metadata plan", systemImage: "terminal")
          .font(.callout)
          .foregroundStyle(.secondary)
      }

      HStack(spacing: 10) {
        TextField("User", text: $user)
          .textFieldStyle(.roundedBorder)
          .frame(width: 180)

        Button {
          Task { _ = await onInspect(user) }
        } label: {
          if isLoading {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Plan SSH", systemImage: "terminal")
          }
        }
        .buttonStyle(.borderedProminent)
        .disabled(isLoading || user.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
      }

      if let error {
        Label(error, systemImage: "exclamationmark.triangle")
          .font(.callout)
          .foregroundStyle(.red)
      } else if let plan {
        SSHPlanSummary(plan: plan)
      } else {
        Text("Plan an SSH command from port-forward or guest-tools metadata without running ssh.")
          .font(.callout)
          .foregroundStyle(.secondary)
      }
    }
    .padding(12)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }
}

private struct NetworkPlanPanel: View {
  var plan: NetworkPlan?
  var isLoading: Bool
  var error: String?
  var onRefresh: () async -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Text("Network Plan")
          .font(.headline)

        Spacer()

        Label("Read only", systemImage: "network")
          .font(.callout)
          .foregroundStyle(.secondary)
      }

      Text(
        "Inspect the planned network metadata without launching the VM or changing live networking."
      )
      .font(.callout)
      .foregroundStyle(.secondary)

      HStack(spacing: 10) {
        Button {
          Task { await onRefresh() }
        } label: {
          if isLoading {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Refresh", systemImage: "arrow.clockwise")
          }
        }
        .buttonStyle(.bordered)
        .disabled(isLoading)

        Button {
          Task { await onRefresh() }
        } label: {
          Label("Inspect", systemImage: "magnifyingglass")
        }
        .buttonStyle(.borderedProminent)
        .disabled(isLoading)
      }

      if let error {
        ErrorLabel(message: error)
      } else if let plan {
        NetworkPlanSummary(plan: plan)
      } else {
        Label("No network plan loaded.", systemImage: "tray")
          .font(.callout)
          .foregroundStyle(.secondary)
      }
    }
    .padding(12)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }
}

private struct OpenPortPlanSummary: View {
  var plan: OpenPortPlan

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 10)], spacing: 10) {
        GuestToolsStatusBadge(title: "Scheme", value: plan.scheme, systemImage: "link")
        GuestToolsStatusBadge(title: "Host", value: plan.host, systemImage: "network")
        GuestToolsStatusBadge(
          title: "Guest Port",
          value: "\(plan.guestPort)",
          systemImage: "arrow.down.forward.circle"
        )
        GuestToolsStatusBadge(
          title: "Host Port",
          value: "\(plan.hostPort)",
          systemImage: "arrow.up.forward.circle"
        )
      }

      GuestToolsFactRow(title: "URL", value: plan.url)
      if !plan.command.isEmpty {
        GuestToolsFactRow(title: "Command", value: plan.command.joined(separator: " "))
      }
    }
  }
}

private struct NetworkPlanSummary: View {
  var plan: NetworkPlan

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 10)], spacing: 10) {
        GuestToolsStatusBadge(title: "Mode", value: plan.mode, systemImage: "bolt.horizontal")
        GuestToolsStatusBadge(title: "Backend", value: plan.backend, systemImage: "server.rack")
        GuestToolsStatusBadge(
          title: "Hostname",
          value: plan.hostname.isEmpty ? "Unavailable" : plan.hostname,
          systemImage: "desktopcomputer"
        )
        GuestToolsStatusBadge(
          title: "Executable",
          value: plan.executable ? "Yes" : "Blocked",
          systemImage: plan.executable ? "checkmark.circle" : "exclamationmark.triangle"
        )
        GuestToolsStatusBadge(
          title: "Forwards",
          value: "\(plan.portForwards.count)",
          systemImage: "point.3.connected.trianglepath.dotted"
        )
      }

      if let capabilities = plan.capabilities {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 160), spacing: 10)], spacing: 10) {
          GuestToolsStatusBadge(
            title: "Guest Outbound",
            value: capabilities.guestOutbound ? "Yes" : "No",
            systemImage: capabilities.guestOutbound ? "checkmark.circle" : "xmark.circle"
          )
          GuestToolsStatusBadge(
            title: "Host To Guest",
            value: capabilities.hostToGuest ? "Yes" : "No",
            systemImage: capabilities.hostToGuest ? "checkmark.circle" : "xmark.circle"
          )
          GuestToolsStatusBadge(
            title: "Port Forwarding",
            value: capabilities.supportsPortForwarding ? "Yes" : "No",
            systemImage: capabilities.supportsPortForwarding ? "checkmark.circle" : "xmark.circle"
          )
          GuestToolsStatusBadge(
            title: "Privileged Helper",
            value: capabilities.requiresPrivilegedHelper ? "Required" : "No",
            systemImage: capabilities.requiresPrivilegedHelper ? "lock" : "lock.open"
          )
        }
      }

      if !plan.portForwards.isEmpty {
        VStack(alignment: .leading, spacing: 6) {
          Text("Port Forwards")
            .font(.caption)
            .foregroundStyle(.secondary)
          ForEach(plan.portForwards) { forward in
            Label("\(forward.host) -> \(forward.guest)", systemImage: "arrow.right.circle")
              .font(.callout)
          }
        }
      }

      if !plan.blockers.isEmpty {
        VStack(alignment: .leading, spacing: 6) {
          Text("Blockers")
            .font(.caption)
            .foregroundStyle(.secondary)
          ForEach(plan.blockers, id: \.code) { blocker in
            VStack(alignment: .leading, spacing: 3) {
              Text(blocker.code)
                .font(.caption.weight(.medium))
              Text(blocker.message)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(8)
            .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
          }
        }
      }

      if !plan.notes.isEmpty {
        GuestToolsFactRow(title: "Notes", value: plan.notes.joined(separator: "\n"))
      }
    }
  }
}

private struct SSHPlanSummary: View {
  var plan: SSHPlan

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 10)], spacing: 10) {
        GuestToolsStatusBadge(
          title: "Source", value: plan.source.title,
          systemImage: "point.3.connected.trianglepath.dotted")
        GuestToolsStatusBadge(title: "User", value: plan.user, systemImage: "person")
        GuestToolsStatusBadge(title: "Host", value: plan.host, systemImage: "network")
        GuestToolsStatusBadge(title: "Port", value: "\(plan.port)", systemImage: "number")
      }

      if !plan.command.isEmpty {
        GuestToolsFactRow(title: "Command", value: plan.commandLine)
      }
    }
  }
}

private struct BootMediaStatusPanel: View {
  var status: BootMediaStatus?
  var isLoading: Bool
  var isImporting: Bool
  var isVerifying: Bool
  var isPlanningDownload: Bool
  var isDownloading: Bool
  var error: String?
  var onRefresh: () async -> Void
  var onImport: (String, BootMediaStatusEntry.Kind?) async -> Bool
  var onVerify: (String, BootMediaStatusEntry.Kind?) async -> Bool
  var onPlanDownload: (String, String?, BootMediaStatusEntry.Kind?) async -> Bool
  var onDownload: (BootMediaStatusEntry.Kind?) async -> Bool

  @State private var sourcePath = ""
  @State private var verificationSHA256 = ""
  @State private var downloadURL = ""
  @State private var downloadExpectedSHA256 = ""
  @State private var selectedKind: BootMediaStatusEntry.Kind? = .installerImage

  private let importKinds: [BootMediaStatusEntry.Kind] = [
    .installerImage,
    .kernel,
    .initrd,
    .macosRestoreImage,
  ]

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Text("Boot Media")
          .font(.headline)

        Spacer()

        Button {
          Task { await onRefresh() }
        } label: {
          if isLoading {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Refresh", systemImage: "arrow.clockwise")
          }
        }
        .disabled(isLoading)
      }

      VStack(alignment: .leading, spacing: 10) {
        HStack(spacing: 10) {
          TextField("/path/to/installer.iso", text: $sourcePath)
            .textFieldStyle(.roundedBorder)

          PathPickerButton(title: "Choose Boot Media", mode: .file, path: $sourcePath)

          Picker("Kind", selection: $selectedKind) {
            Text("Auto").tag(Optional<BootMediaStatusEntry.Kind>.none)
            ForEach(importKinds, id: \.self) { kind in
              Text(kind.title).tag(Optional(kind))
            }
          }
          .frame(width: 190)

          Button {
            Task {
              let imported = await onImport(trimmedSourcePath, selectedKind)
              if imported {
                sourcePath = ""
              }
            }
          } label: {
            if isImporting {
              ProgressView()
                .controlSize(.small)
            } else {
              Label("Import", systemImage: "square.and.arrow.down")
            }
          }
          .buttonStyle(.borderedProminent)
          .disabled(isImporting || trimmedSourcePath.isEmpty)
        }

        HStack(spacing: 10) {
          TextField("Expected SHA256", text: $verificationSHA256)
            .textFieldStyle(.roundedBorder)

          Button {
            Task {
              let verified = await onVerify(trimmedVerificationSHA256, selectedKind)
              if verified {
                verificationSHA256 = ""
              }
            }
          } label: {
            if isVerifying {
              ProgressView()
                .controlSize(.small)
            } else {
              Label("Verify", systemImage: "checkmark.shield")
            }
          }
          .disabled(isVerifying || !isVerificationSHA256Valid)
        }

        HStack(spacing: 10) {
          TextField("https://example.com/installer.iso", text: $downloadURL)
            .textFieldStyle(.roundedBorder)

          TextField("SHA256 optional", text: $downloadExpectedSHA256)
            .textFieldStyle(.roundedBorder)
            .frame(width: 180)

          Button {
            Task {
              let planned = await onPlanDownload(
                trimmedDownloadURL,
                trimmedDownloadExpectedSHA256.isEmpty ? nil : trimmedDownloadExpectedSHA256,
                selectedKind
              )
              if planned {
                downloadURL = ""
                downloadExpectedSHA256 = ""
              }
            }
          } label: {
            if isPlanningDownload {
              ProgressView()
                .controlSize(.small)
            } else {
              Label("Plan Download", systemImage: "list.bullet.clipboard")
            }
          }
          .disabled(
            isPlanningDownload
              || !isDownloadURLValid
              || !isDownloadExpectedSHA256Valid)
        }

        HStack(spacing: 10) {
          Text("Execute the most recent planned download for the selected kind.")
            .font(.callout)
            .foregroundStyle(.secondary)

          Spacer()

          Button {
            Task {
              _ = await onDownload(selectedKind)
            }
          } label: {
            if isDownloading {
              ProgressView()
                .controlSize(.small)
            } else {
              Label("Execute Planned Download", systemImage: "arrow.down.circle")
            }
          }
          .disabled(isDownloading || !hasPlannedDownload)
        }
      }
      .padding(12)
      .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))

      if let error {
        Label(error, systemImage: "exclamationmark.triangle")
          .font(.callout)
          .foregroundStyle(.red)
      } else if let status {
        if status.entries.isEmpty {
          Text("No boot media entries reported.")
            .font(.callout)
            .foregroundStyle(.secondary)
        } else {
          VStack(spacing: 8) {
            ForEach(status.entries) { entry in
              BootMediaStatusRow(entry: entry)
            }
          }
        }
      } else {
        Text("Refresh to inspect Fast Mode boot media for this VM.")
          .font(.callout)
          .foregroundStyle(.secondary)
      }
    }
  }

  private var hasPlannedDownload: Bool {
    status?.entries.contains { entry in
      guard entry.lastDownloadPlan != nil else {
        return false
      }
      guard let selectedKind else {
        return true
      }
      return entry.kind == selectedKind
    } ?? false
  }

  private var trimmedSourcePath: String {
    sourcePath.trimmingCharacters(in: .whitespacesAndNewlines)
  }

  private var trimmedVerificationSHA256: String {
    verificationSHA256.trimmingCharacters(in: .whitespacesAndNewlines)
  }

  private var trimmedDownloadURL: String {
    downloadURL.trimmingCharacters(in: .whitespacesAndNewlines)
  }

  private var trimmedDownloadExpectedSHA256: String {
    downloadExpectedSHA256.trimmingCharacters(in: .whitespacesAndNewlines)
  }

  private var isVerificationSHA256Valid: Bool {
    isSHA256Hex(trimmedVerificationSHA256)
  }

  private var isDownloadURLValid: Bool {
    URL(string: trimmedDownloadURL)?.scheme?.lowercased() == "https"
  }

  private var isDownloadExpectedSHA256Valid: Bool {
    trimmedDownloadExpectedSHA256.isEmpty || isSHA256Hex(trimmedDownloadExpectedSHA256)
  }

  private func isSHA256Hex(_ value: String) -> Bool {
    value.count == 64 && value.allSatisfy(\.isHexDigit)
  }
}

private struct BootMediaStatusRow: View {
  var entry: BootMediaStatusEntry

  var body: some View {
    HStack(alignment: .top, spacing: 12) {
      Image(systemName: entry.exists ? "checkmark.circle.fill" : "xmark.circle")
        .font(.title3)
        .foregroundStyle(entry.exists ? .green : .secondary)
        .frame(width: 26)

      VStack(alignment: .leading, spacing: 6) {
        HStack(alignment: .firstTextBaseline) {
          Text(entry.kind.title)
            .font(.headline)

          Spacer()

          Text(entry.exists ? "Present" : "Missing")
            .font(.caption.weight(.medium))
            .foregroundStyle(entry.exists ? .green : .secondary)
        }

        Text(entry.path)
          .font(.callout)
          .foregroundStyle(.secondary)
          .lineLimit(1)
          .truncationMode(.middle)

        HStack(spacing: 16) {
          Label(sizeText, systemImage: "internaldrive")
          Label(verificationText, systemImage: "checkmark.shield")
          Label(downloadText, systemImage: "arrow.down.circle")
        }
        .font(.caption)
        .foregroundStyle(.secondary)
      }
    }
    .padding(12)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private var sizeText: String {
    guard let sizeBytes = entry.sizeBytes else {
      return "Size unknown"
    }
    return ByteCountFormatter.string(fromByteCount: Int64(sizeBytes), countStyle: .file)
  }

  private var verificationText: String {
    guard let verification = entry.lastVerification else {
      return "Not verified"
    }
    return verification.verified ? "Verified" : "Verification failed"
  }

  private var downloadText: String {
    if let download = entry.lastDownload {
      return download.downloaded ? "Downloaded" : "Download failed"
    }
    if entry.lastDownloadPlan != nil {
      return "Download planned"
    }
    return "No download"
  }
}

private struct SnapshotPreflightPanel: View {
  var status: SnapshotPreflightStatus?
  var isLoading: Bool
  var error: String?
  var execution: ApplicationConsistentSnapshotExecution?
  var isExecuting: Bool
  var executionError: String?
  var onRefresh: () async -> Void
  var onExecute: (String, UInt64?) async -> Bool

  @State private var snapshotName = "app-consistent-snapshot"
  @State private var freezeTimeoutMillis = "5000"

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Text("Snapshot Consistency")
          .font(.headline)

        Spacer()

        Button {
          Task { await onRefresh() }
        } label: {
          if isLoading {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Refresh", systemImage: "arrow.clockwise")
          }
        }
        .disabled(isLoading)
      }

      if let error {
        Label(error, systemImage: "exclamationmark.triangle")
          .font(.callout)
          .foregroundStyle(.red)
      } else if let status {
        VStack(alignment: .leading, spacing: 10) {
          LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 14)], spacing: 14) {
            GuestToolsStatusBadge(
              title: "Mode",
              value: status.consistency.title,
              systemImage: "camera.metering.matrix"
            )
            GuestToolsStatusBadge(
              title: "Preflight",
              value: status.readinessTitle,
              systemImage: status.backendFreezeThawSupported ? "checkmark.shield" : "hammer"
            )
            GuestToolsStatusBadge(
              title: "Guest Tools",
              value: status.guestToolsConnected ? "Connected" : "Not connected",
              systemImage: status.guestToolsConnected ? "link" : "link.badge.plus"
            )
            GuestToolsStatusBadge(
              title: "Capabilities",
              value: "\(status.capabilities.count)",
              systemImage: "checklist"
            )
          }

          if !status.backendFreezeThawSupported {
            Label(
              "Daemon freeze/thaw execution is available after preflight reports backend support.",
              systemImage: "info.circle"
            )
            .font(.callout)
            .foregroundStyle(.secondary)
          }

          GuestToolsFactRow(title: "Checked", value: unixTimeText(status.checkedAtUnix))
          SnapshotCapabilityList(capabilities: status.capabilities)
          SnapshotBlockerList(blockers: status.blockers)
          SnapshotExecutionControls(
            snapshotName: $snapshotName,
            freezeTimeoutMillis: $freezeTimeoutMillis,
            isReady: status.ready,
            isExecuting: isExecuting,
            onExecute: executeSnapshot
          )
          if let executionError {
            Label(executionError, systemImage: "exclamationmark.triangle")
              .font(.callout)
              .foregroundStyle(.red)
          }
          if let execution {
            SnapshotExecutionSummary(execution: execution)
          }
        }
      } else {
        Text(
          "Refresh to inspect application-consistent snapshot preflight metadata before running daemon freeze/thaw execution."
        )
        .font(.callout)
        .foregroundStyle(.secondary)
      }
    }
    .padding(12)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private func unixTimeText(_ value: UInt64?) -> String {
    guard let value else {
      return "Not reported"
    }
    return Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }

  private func executeSnapshot() async {
    let timeout = UInt64(freezeTimeoutMillis.trimmingCharacters(in: .whitespacesAndNewlines))
    _ = await onExecute(snapshotName, timeout)
  }
}

private struct SnapshotExecutionControls: View {
  @Binding var snapshotName: String
  @Binding var freezeTimeoutMillis: String
  var isReady: Bool
  var isExecuting: Bool
  var onExecute: () async -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      HStack(alignment: .firstTextBaseline, spacing: 8) {
        TextField("Snapshot name", text: $snapshotName)
          .textFieldStyle(.roundedBorder)
        TextField("Freeze timeout ms", text: $freezeTimeoutMillis)
          .textFieldStyle(.roundedBorder)
          .frame(width: 150)
        Button {
          Task { await onExecute() }
        } label: {
          if isExecuting {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Execute", systemImage: "camera.aperture")
          }
        }
        .buttonStyle(.borderedProminent)
        .disabled(!canExecute)
      }

      if !isReady {
        Text("Resolve preflight blockers before executing application-consistent snapshot.")
          .font(.caption)
          .foregroundStyle(.secondary)
      }
    }
  }

  private var canExecute: Bool {
    let trimmedName = snapshotName.trimmingCharacters(in: .whitespacesAndNewlines)
    let trimmedTimeout = freezeTimeoutMillis.trimmingCharacters(in: .whitespacesAndNewlines)
    return isReady && !isExecuting && !trimmedName.isEmpty
      && (trimmedTimeout.isEmpty || UInt64(trimmedTimeout) != nil)
  }
}

private struct SnapshotExecutionSummary: View {
  var execution: ApplicationConsistentSnapshotExecution

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Label(execution.summaryTitle, systemImage: "checkmark.seal")
        .font(.callout.weight(.medium))
      GuestToolsFactRow(title: "Snapshot", value: execution.snapshot)
      GuestToolsFactRow(title: "Created", value: unixTimeText(execution.snapshotCreatedAtUnix))
      GuestToolsFactRow(title: "Freeze", value: commandText(execution.freezeResult))
      GuestToolsFactRow(title: "Thaw", value: commandText(execution.thawResult))
      GuestToolsFactRow(
        title: "Pending",
        value:
          "\(execution.pendingCommandsAfterFreeze) after freeze / \(execution.pendingCommandsAfterThaw) after thaw"
      )
      Text(execution.note)
        .font(.caption)
        .foregroundStyle(.secondary)
        .fixedSize(horizontal: false, vertical: true)
    }
    .padding(10)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private func commandText(_ result: ApplicationConsistentSnapshotCommandResult) -> String {
    let capability = result.capability ?? "no capability"
    let message = result.message ?? result.statusTitle
    return "\(result.requestID) - \(capability) - \(message)"
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct SnapshotCapabilityList: View {
  var capabilities: [String]

  var body: some View {
    if capabilities.isEmpty {
      Text("No snapshot capabilities reported.")
        .font(.callout)
        .foregroundStyle(.secondary)
    } else {
      LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 8)], spacing: 8) {
        ForEach(capabilities, id: \.self) { capability in
          Text(capability)
            .font(.caption.weight(.medium))
            .lineLimit(1)
            .frame(maxWidth: .infinity, minHeight: 34, alignment: .leading)
            .padding(8)
            .background(
              Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
        }
      }
    }
  }
}

private struct SnapshotBlockerList: View {
  var blockers: [SnapshotPreflightBlocker]

  var body: some View {
    if blockers.isEmpty {
      Label("No preflight blockers reported.", systemImage: "checkmark.circle")
        .font(.callout)
        .foregroundStyle(.secondary)
    } else {
      VStack(alignment: .leading, spacing: 8) {
        ForEach(blockers) { blocker in
          VStack(alignment: .leading, spacing: 3) {
            Text(blocker.code)
              .font(.caption.weight(.medium))
            Text(blocker.message)
              .font(.caption)
              .foregroundStyle(.secondary)
              .fixedSize(horizontal: false, vertical: true)
            if let path = blocker.path {
              Text(path)
                .font(.caption2.monospaced())
                .foregroundStyle(.secondary)
                .lineLimit(2)
            }
          }
          .frame(maxWidth: .infinity, alignment: .leading)
          .padding(8)
          .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
        }
      }
    }
  }
}

private struct RunnerStatusPanel: View {
  var status: RunnerStatus?
  var preRunLaunchReadiness: LaunchReadiness?
  var isLoading: Bool
  var error: String?
  var runtimeControlResult: RuntimeControlCommandResult?
  var isSendingRuntimeControl: Bool
  var runtimeControlError: String?
  var onPrepare: () async -> Bool
  var onRefresh: () async -> Void
  var onRuntimeControlStatus: () async -> Bool
  var onRuntimeControlStopDisplay: () async -> Bool
  var onRuntimeControlPolicy: () async -> Bool
  var onRuntimeControlPacing: () async -> Bool

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Text("Launch Readiness")
          .font(.headline)

        Spacer()

        Button {
          Task { _ = await onPrepare() }
        } label: {
          if isLoading {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Prepare Run", systemImage: "doc.text.magnifyingglass")
          }
        }
        .disabled(isLoading)

        Button {
          Task { await onRefresh() }
        } label: {
          if isLoading {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Refresh", systemImage: "arrow.clockwise")
          }
        }
        .disabled(isLoading)
      }

      if let error {
        Label(error, systemImage: "exclamationmark.triangle")
          .font(.callout)
          .foregroundStyle(.red)
      } else if let status {
        VStack(alignment: .leading, spacing: 10) {
          LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 14)], spacing: 14) {
            GuestToolsStatusBadge(
              title: "Engine",
              value: status.engine,
              systemImage: status.engine == "lightvm" ? "bolt.horizontal" : "gearshape.2"
            )
            GuestToolsStatusBadge(
              title: "Mode",
              value: status.dryRun ? "Dry run" : "Running",
              systemImage: status.dryRun ? "doc.text.magnifyingglass" : "play.fill"
            )
            GuestToolsStatusBadge(
              title: "Ready",
              value: status.launchReadinessTitle,
              systemImage: status.launchReadiness?.ready == true
                ? "checkmark.circle" : "exclamationmark.triangle"
            )
          }

          VStack(alignment: .leading, spacing: 6) {
            GuestToolsFactRow(title: "PID", value: status.pid.map(String.init) ?? "None")
            GuestToolsFactRow(title: "Command", value: status.commandLine)
            GuestToolsFactRow(title: "Log", value: status.logPath)
            if let launchSpecPath = status.launchSpecPath {
              GuestToolsFactRow(title: "Launch Spec", value: launchSpecPath)
            }
            if let supervisor = status.qmpSupervisor {
              GuestToolsFactRow(
                title: "QMP Supervisor",
                value:
                  "\(supervisor.summaryTitle); envelopes \(supervisor.envelopesRead)"
                    + (supervisor.limitReached ? "; limit reached" : "")
              )
            }
            if let guestTools = status.guestTools {
              GuestToolsFactRow(title: "Guest Tools Transport", value: guestTools.transport)
              GuestToolsFactRow(title: "Guest Tools Channel", value: guestTools.channelName)
              GuestToolsFactRow(title: "Guest Tools Socket", value: guestTools.socketPath)
              GuestToolsFactRow(title: "Guest Tools Token", value: guestTools.tokenPath)
            }
            if let runtimeControl = status.runtimeControl {
              GuestToolsFactRow(title: "Display Control", value: runtimeControl.kind)
              GuestToolsFactRow(title: "Display Control Socket", value: runtimeControl.socketPath)
              GuestToolsFactRow(
                title: "Display Control Commands",
                value: runtimeControl.commandSummary
              )
            }
            GuestToolsFactRow(
              title: status.dryRun ? "Metadata Recorded" : "Started",
              value: unixTimeText(status.startedAtUnix)
            )
          }

          if let runtimeControl = status.runtimeControl {
            HStack(spacing: 8) {
              if supports(command: "status", in: runtimeControl) {
                Button {
                  Task { _ = await onRuntimeControlStatus() }
                } label: {
                  if isSendingRuntimeControl {
                    ProgressView()
                      .controlSize(.small)
                  } else {
                    Label("Status", systemImage: "waveform.path.ecg")
                  }
                }
                .disabled(isLoading || isSendingRuntimeControl)
                .help("Ask the embedded Apple VZ display runner for its current status")
              }

              if supports(command: "policy", in: runtimeControl) {
                Button {
                  Task { _ = await onRuntimeControlPolicy() }
                } label: {
                  if isSendingRuntimeControl {
                    ProgressView()
                      .controlSize(.small)
                  } else {
                    Label("Policy", systemImage: "slider.horizontal.3")
                  }
                }
                .disabled(isLoading || isSendingRuntimeControl)
                .help("Ask the embedded Apple VZ display runner for its latest runtime policy")
              }

              if supports(command: "pacing", in: runtimeControl) {
                Button {
                  Task { _ = await onRuntimeControlPacing() }
                } label: {
                  if isSendingRuntimeControl {
                    ProgressView()
                      .controlSize(.small)
                  } else {
                    Label("Pacing", systemImage: "speedometer")
                  }
                }
                .disabled(isLoading || isSendingRuntimeControl)
                .help("Ask the embedded Apple VZ display runner for policy-derived display pacing")
              }

              if supports(command: "stop", in: runtimeControl) {
                Button(role: .destructive) {
                  Task { _ = await onRuntimeControlStopDisplay() }
                } label: {
                  if isSendingRuntimeControl {
                    ProgressView()
                      .controlSize(.small)
                  } else {
                    Label("Stop Display", systemImage: "xmark.circle")
                  }
                }
                .disabled(isLoading || isSendingRuntimeControl)
                .help("Ask the embedded Apple VZ display runner to stop")
              }
            }
            .controlSize(.small)
          }

          runtimeControlFeedback

          if let readiness = status.launchReadiness {
            ReadinessBlockerList(readiness: readiness)
          }
        }
      } else if let preRunLaunchReadiness {
        VStack(alignment: .leading, spacing: 10) {
          LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 14)], spacing: 14) {
            GuestToolsStatusBadge(
              title: "Source",
              value: "Pre-run report",
              systemImage: "checklist"
            )
            GuestToolsStatusBadge(
              title: "Ready",
              value: preRunLaunchReadiness.title,
              systemImage: preRunLaunchReadiness.ready
                ? "checkmark.circle" : "exclamationmark.triangle"
            )
          }

          Text("Runner metadata has not been prepared yet; showing pre-run launch readiness from the aggregate report.")
            .font(.callout)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)

          ReadinessBlockerList(readiness: preRunLaunchReadiness)
          runtimeControlFeedback
        }
      } else {
        VStack(alignment: .leading, spacing: 10) {
          Text(
            "Refresh to inspect daemon runner metadata, command plan, and Fast Mode launch blockers."
          )
          .font(.callout)
          .foregroundStyle(.secondary)

          runtimeControlFeedback
        }
      }
    }
    .padding(12)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  @ViewBuilder
  private var runtimeControlFeedback: some View {
    if let runtimeControlError {
      Label(runtimeControlError, systemImage: "exclamationmark.triangle")
        .font(.callout)
        .foregroundStyle(.red)
    }

    if let runtimeControlResult {
      VStack(alignment: .leading, spacing: 6) {
        GuestToolsFactRow(
          title: "Last Display Control",
          value: runtimeControlResult.command
        )
        GuestToolsFactRow(
          title: "Display Response",
          value: runtimeControlResult.responseSummary
        )
      }
    }
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }

  private func supports(command: String, in runtimeControl: RuntimeControlRunnerStatus) -> Bool {
    runtimeControl.commands.contains { $0.caseInsensitiveCompare(command) == .orderedSame }
  }
}

private struct RuntimeResourcePolicyPanel: View {
  var policy: RuntimeResourcePolicy?
  var isApplying: Bool
  var error: String?
  var onReapply: (RuntimeResourceVisibility) async -> Bool

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Text("Runtime Resources")
          .font(.headline)

        Spacer()

        ForEach(RuntimeResourceVisibility.allCases, id: \.self) { visibility in
          Button {
            Task { _ = await onReapply(visibility) }
          } label: {
            if isApplying {
              ProgressView()
                .controlSize(.small)
            } else {
              Label(visibility.title, systemImage: visibility.systemImage)
            }
          }
          .disabled(isApplying)
        }
      }

      if let error {
        Label(error, systemImage: "exclamationmark.triangle")
          .font(.callout)
          .foregroundStyle(.red)
      } else if let policy {
        VStack(alignment: .leading, spacing: 10) {
          LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 14)], spacing: 14) {
            GuestToolsStatusBadge(
              title: "Visibility",
              value: policy.visibility.title,
              systemImage: policy.visibility.systemImage
            )
            GuestToolsStatusBadge(
              title: "Power",
              value: policy.onBattery ? "Battery" : "AC",
              systemImage: policy.onBattery ? "battery.50" : "powerplug"
            )
            GuestToolsStatusBadge(
              title: "Live Apply",
              value: policy.liveApplyTitle,
              systemImage: policy.liveApplied ? "checkmark.circle" : "record.circle"
            )
            GuestToolsStatusBadge(
              title: "Policy Ack",
              value: policy.runtimeControlAcknowledged ? "Display helper" : "Metadata only",
              systemImage: policy.runtimeControlAcknowledged ? "checkmark.circle" : "doc.text"
            )
          }

          VStack(alignment: .leading, spacing: 6) {
            GuestToolsFactRow(title: "Profile", value: policy.profile)
            GuestToolsFactRow(title: "Memory", value: policy.memory)
            GuestToolsFactRow(title: "CPU", value: policy.cpu)
            GuestToolsFactRow(title: "Display FPS", value: policy.displayFPSCap)
            GuestToolsFactRow(title: "Rationale", value: policy.rationale)
            GuestToolsFactRow(title: "Metadata Recorded", value: unixTimeText(policy.updatedAtUnix))
          }

          if !policy.liveApplied && !policy.liveApplyBlockers.isEmpty {
            Label("Policy recorded only; live runtime control is unavailable.", systemImage: "record.circle")
              .font(.caption)
              .foregroundStyle(.secondary)
          }

          ForEach(policy.liveApplyBlockers, id: \.code) { blocker in
            VStack(alignment: .leading, spacing: 3) {
              Text(blocker.code)
                .font(.caption.weight(.medium))
              Text(blocker.message)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(8)
            .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
          }
        }
      } else {
        Text("No runtime policy recorded.")
          .font(.callout)
          .foregroundStyle(.secondary)
      }
    }
    .padding(12)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct QemuLaunchPlanPanel: View {
  var plan: QemuLaunchPlan?
  var isLoading: Bool
  var error: String?
  var onRefresh: () async -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Text("QEMU Launch Plan")
          .font(.headline)

        Spacer()

        Button {
          Task { await onRefresh() }
        } label: {
          if isLoading {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Refresh", systemImage: "terminal")
          }
        }
        .disabled(isLoading)
      }

      if let error {
        Label(error, systemImage: "exclamationmark.triangle")
          .font(.callout)
          .foregroundStyle(.red)
      } else if let plan {
        VStack(alignment: .leading, spacing: 10) {
          LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 14)], spacing: 14) {
            GuestToolsStatusBadge(
              title: "Program",
              value: plan.program,
              systemImage: "terminal"
            )
            GuestToolsStatusBadge(
              title: "Arguments",
              value: "\(plan.args.count)",
              systemImage: "list.bullet.rectangle"
            )
            GuestToolsStatusBadge(
              title: "Network",
              value: plan.networkSummary,
              systemImage: "network"
            )
            GuestToolsStatusBadge(
              title: "Viewer",
              value: viewerSummary,
              systemImage: "display"
            )
          }

          Text(plan.commandLine)
            .font(.system(.caption, design: .monospaced))
            .textSelection(.enabled)
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
        }
      } else {
        Text(
          "Refresh to inspect the daemon-rendered Compatibility Mode QEMU command without launching the VM."
        )
        .font(.callout)
        .foregroundStyle(.secondary)
      }
    }
    .padding(12)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private var viewerSummary: String {
    guard let plan else {
      return "Not loaded"
    }

    return plan.viewerEndpoint?.absoluteString ?? "Diagnostics only"
  }
}

private struct SnapshotMetadataPanel: View {
  var snapshots: [VMSnapshot]
  var isLoading: Bool
  var error: String?
  var chain: VMSnapshotChain?
  var isLoadingChain: Bool
  var chainError: String?
  var restoreResult: SnapshotRestoreResult?
  var isRestoring: Bool
  var restoreError: String?
  var snapshotCreation: VMSnapshot?
  var isCreatingSnapshot: Bool
  var snapshotCreationError: String?
  var diskCreation: VMSnapshotDiskCreation?
  var isCreatingDisk: Bool
  var diskCreationError: String?
  var onRefresh: () async -> Void
  var onRefreshChain: () async -> Void
  var onRestore: (String) async -> Bool
  var onCreateSnapshot: (String, VMSnapshotKind) async -> Bool
  var onCreateDisk: (String) async -> Bool

  @State private var selectedSnapshotName = ""
  @State private var selectedSnapshotKind = VMSnapshotKind.disk
  @State private var restoreConfirmationSnapshotName = ""
  @State private var isRestoreConfirmationPresented = false

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Text("Snapshots")
          .font(.headline)

        Spacer()

        Button {
          Task { await onRefresh() }
        } label: {
          if isLoading {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Refresh", systemImage: "arrow.clockwise")
          }
        }
        .disabled(isLoading)

        Button {
          Task { await onRefreshChain() }
        } label: {
          if isLoadingChain {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Chain", systemImage: "externaldrive.connected.to.line.below")
          }
        }
        .disabled(isLoadingChain)
      }

      VStack(alignment: .leading, spacing: 10) {
        TextField("Snapshot name", text: $selectedSnapshotName)
          .textFieldStyle(.roundedBorder)

        HStack(spacing: 10) {
          Picker("Kind", selection: $selectedSnapshotKind) {
            ForEach(VMSnapshotKind.creatableKinds, id: \.self) { kind in
              Text(kind.title).tag(kind)
            }
          }
          .pickerStyle(.segmented)

          Button {
            Task { _ = await onCreateSnapshot(selectedSnapshotName, selectedSnapshotKind) }
          } label: {
            if isCreatingSnapshot {
              ProgressView()
                .controlSize(.small)
            } else {
              Label("Create Metadata", systemImage: "camera.badge.plus")
            }
          }
          .buttonStyle(.borderedProminent)
          .disabled(
            isCreatingSnapshot
              || selectedSnapshotName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
      }

      if let error {
        Label(error, systemImage: "exclamationmark.triangle")
          .font(.callout)
          .foregroundStyle(.red)
      } else if snapshots.isEmpty {
        Text("Refresh to inspect recorded snapshot metadata for this VM.")
          .font(.callout)
          .foregroundStyle(.secondary)
      } else {
        VStack(alignment: .leading, spacing: 10) {
          ForEach(snapshots) { snapshot in
            SnapshotMetadataRow(
              snapshot: snapshot,
              isSelected: selectedSnapshotName == snapshot.name,
              onSelect: {
                selectedSnapshotName = snapshot.name
              }
            )
          }

          HStack(spacing: 10) {
            Button {
              restoreConfirmationSnapshotName =
                selectedSnapshotName.trimmingCharacters(in: .whitespacesAndNewlines)
              isRestoreConfirmationPresented = true
            } label: {
              if isRestoring {
                ProgressView()
                  .controlSize(.small)
              } else {
                Label("Restore Metadata", systemImage: "arrow.uturn.backward.circle")
              }
            }
            .buttonStyle(.borderedProminent)
            .disabled(
              isRestoring
                || selectedSnapshotName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
          }

          Button {
            Task { _ = await onCreateDisk(selectedSnapshotName) }
          } label: {
            if isCreatingDisk {
              ProgressView()
                .controlSize(.small)
            } else {
              Label("Create Disk", systemImage: "externaldrive.badge.plus")
            }
          }
          .buttonStyle(.bordered)
          .disabled(
            isCreatingDisk
              || selectedSnapshotName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
      }

      if let restoreError {
        Label(restoreError, systemImage: "exclamationmark.triangle")
          .font(.callout)
          .foregroundStyle(.red)
      }

      if let snapshotCreationError {
        Label(snapshotCreationError, systemImage: "exclamationmark.triangle")
          .font(.callout)
          .foregroundStyle(.red)
      }

      if let diskCreationError {
        Label(diskCreationError, systemImage: "exclamationmark.triangle")
          .font(.callout)
          .foregroundStyle(.red)
      }

      if let chainError {
        Label(chainError, systemImage: "exclamationmark.triangle")
          .font(.callout)
          .foregroundStyle(.red)
      }

      if let chain {
        SnapshotChainSummary(chain: chain)
      }

      if let restoreResult {
        SnapshotRestoreSummary(result: restoreResult)
      }

      if let snapshotCreation {
        SnapshotCreationSummary(snapshot: snapshotCreation)
      }

      if let diskCreation {
        SnapshotDiskCreationSummary(creation: diskCreation)
      }
    }
    .padding(12)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
    .alert("Restore snapshot metadata?", isPresented: $isRestoreConfirmationPresented) {
      Button("Cancel", role: .cancel) {}
      Button("Restore", role: .destructive) {
        let snapshotName = restoreConfirmationSnapshotName
        restoreConfirmationSnapshotName = ""
        Task { _ = await onRestore(snapshotName) }
      }
    } message: {
      Text(
        "Restore metadata from \(restoreConfirmationSnapshotName). This may change the active disk and VM state recorded in metadata."
      )
    }
  }
}

extension VMSnapshotKind {
  fileprivate static var creatableKinds: [VMSnapshotKind] {
    [.disk, .suspend, .applicationConsistent]
  }
}

private struct SnapshotMetadataRow: View {
  var snapshot: VMSnapshot
  var isSelected: Bool
  var onSelect: () -> Void

  var body: some View {
    Button(action: onSelect) {
      HStack(alignment: .top, spacing: 12) {
        Image(systemName: isSelected ? "largecircle.fill.circle" : "circle")
          .font(.title3)
          .foregroundStyle(isSelected ? Color.accentColor : Color.secondary)
          .frame(width: 26)

        VStack(alignment: .leading, spacing: 6) {
          HStack(alignment: .firstTextBaseline) {
            Text(snapshot.name)
              .font(.headline)
            Spacer()
            Text(snapshot.kind.title)
              .font(.caption.weight(.medium))
              .foregroundStyle(.secondary)
          }

          HStack(spacing: 16) {
            Label(snapshot.vmState.title, systemImage: "power")
            Label(unixTimeText(snapshot.createdAtUnix), systemImage: "clock")
          }
          .font(.caption)
          .foregroundStyle(.secondary)
        }
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      .padding(12)
      .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct SnapshotChainSummary: View {
  var chain: VMSnapshotChain

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      Label(
        chain.readinessTitle,
        systemImage: chain.activeDisk.exists ? "link.circle" : "link.badge.plus"
      )
      .font(.callout.weight(.medium))
      .foregroundStyle(.secondary)

      VStack(alignment: .leading, spacing: 6) {
        GuestToolsFactRow(title: "Active source", value: chain.activeDisk.sourceTitle)
        if let snapshot = chain.activeDisk.snapshot {
          GuestToolsFactRow(title: "Active snapshot", value: snapshot)
        }
        GuestToolsFactRow(title: "Active disk", value: chain.activeDisk.path)
        GuestToolsFactRow(title: "Format", value: chain.activeDisk.format)
        GuestToolsFactRow(title: "Ready", value: chain.activeDisk.exists ? "true" : "false")
        GuestToolsFactRow(title: "Activated", value: unixTimeText(chain.activeDisk.activatedAtUnix))
      }

      if chain.disks.isEmpty {
        Text("No disk snapshot chain metadata recorded yet.")
          .font(.callout)
          .foregroundStyle(.secondary)
      } else {
        VStack(alignment: .leading, spacing: 8) {
          ForEach(chain.disks) { disk in
            SnapshotDiskRow(disk: disk)
          }
        }
      }
    }
    .padding(10)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct SnapshotDiskRow: View {
  var disk: VMSnapshotDisk

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      HStack(alignment: .firstTextBaseline) {
        Text(disk.snapshot)
          .font(.caption.weight(.semibold))
        Spacer()
        Text(disk.overlayExists ? "Overlay ready" : "Overlay planned")
          .font(.caption)
          .foregroundStyle(disk.overlayExists ? Color.secondary : Color.orange)
      }

      GuestToolsFactRow(title: "Overlay", value: disk.overlayPath)
      GuestToolsFactRow(title: "Backing", value: disk.backingPath)
      GuestToolsFactRow(title: "Backing ready", value: disk.backingExists ? "true" : "false")
      GuestToolsFactRow(title: "Create command", value: disk.createCommandLine)
      GuestToolsFactRow(title: "Prepared", value: unixTimeText(disk.preparedAtUnix))
    }
    .padding(8)
    .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct SnapshotRestoreSummary: View {
  var result: SnapshotRestoreResult

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Label("Snapshot metadata restored.", systemImage: "checkmark.circle")
        .font(.callout)
        .foregroundStyle(.secondary)

      GuestToolsFactRow(title: "Snapshot", value: result.snapshot)
      GuestToolsFactRow(title: "State", value: result.restoredState.title)
      GuestToolsFactRow(title: "Restored", value: unixTimeText(result.restoredAtUnix))

      if let activeDisk = result.activeDisk {
        GuestToolsFactRow(title: "Active disk", value: activeDisk.path)
        GuestToolsFactRow(title: "Disk source", value: activeDisk.source)
        GuestToolsFactRow(title: "Disk ready", value: activeDisk.exists ? "true" : "false")
      }

      if let suspendImage = result.suspendImage {
        GuestToolsFactRow(title: "Suspend image", value: suspendImage.imagePath)
        GuestToolsFactRow(
          title: "Image ready",
          value: suspendImage.imageExists ? "true" : "false"
        )
      }
    }
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct SnapshotCreationSummary: View {
  var snapshot: VMSnapshot

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Label("Snapshot metadata created.", systemImage: "checkmark.circle")
        .font(.callout)
        .foregroundStyle(.secondary)

      GuestToolsFactRow(title: "Snapshot", value: snapshot.name)
      GuestToolsFactRow(title: "Kind", value: snapshot.kind.title)
      GuestToolsFactRow(title: "State", value: snapshot.vmState.title)
      GuestToolsFactRow(title: "Created", value: unixTimeText(snapshot.createdAtUnix))
    }
    .padding(10)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct SnapshotDiskCreationSummary: View {
  var creation: VMSnapshotDiskCreation

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Label(
        creation.executed ? "Snapshot disk command finished." : "Snapshot disk already ready.",
        systemImage: creation.executed ? "checkmark.circle" : "externaldrive.badge.checkmark"
      )
      .font(.callout)
      .foregroundStyle(.secondary)

      GuestToolsFactRow(title: "Snapshot", value: creation.snapshot)
      GuestToolsFactRow(title: "Overlay", value: creation.disk.overlayPath)
      GuestToolsFactRow(
        title: "Overlay ready", value: creation.disk.overlayExists ? "true" : "false")
      GuestToolsFactRow(title: "Backing", value: creation.disk.backingPath)
      GuestToolsFactRow(
        title: "Backing ready", value: creation.disk.backingExists ? "true" : "false")
      GuestToolsFactRow(title: "Command", value: creation.commandLine)
      GuestToolsFactRow(title: "Executed", value: creation.executed ? "true" : "false")
      if let exitStatus = creation.exitStatus {
        GuestToolsFactRow(title: "Exit status", value: exitStatus)
      }
      if !creation.stdout.isEmpty {
        GuestToolsFactRow(title: "stdout", value: creation.stdout)
      }
      if !creation.stderr.isEmpty {
        GuestToolsFactRow(title: "stderr", value: creation.stderr)
      }
      GuestToolsFactRow(title: "Created", value: unixTimeText(creation.createdAtUnix))
    }
    .padding(10)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct StorageMaintenancePanel: View {
  var preparation: DiskPreparation?
  var isPreparing: Bool
  var preparationError: String?
  var creation: VMDiskCreation?
  var isCreating: Bool
  var creationError: String?
  var inspection: VMDiskInspection?
  var isInspecting: Bool
  var inspectionError: String?
  var verification: VMDiskVerification?
  var isVerifying: Bool
  var verificationError: String?
  var compaction: VMDiskCompaction?
  var isCompacting: Bool
  var compactionError: String?
  var onPrepare: () async -> Bool
  var onCreate: () async -> Bool
  var onInspect: () async -> Bool
  var onVerify: () async -> Bool
  var onCompact: () async -> Bool
  @State private var isCreateConfirmationPresented = false
  @State private var isCompactConfirmationPresented = false

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Text("Storage Maintenance")
          .font(.headline)

        Spacer()

        Button {
          Task { _ = await onPrepare() }
        } label: {
          if isPreparing {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Prepare", systemImage: "externaldrive")
          }
        }
        .disabled(isBusy)

        Button {
          isCreateConfirmationPresented = true
        } label: {
          if isCreating {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Create Disk", systemImage: "externaldrive.badge.plus")
          }
        }
        .disabled(isBusy)

        Button {
          Task { _ = await onInspect() }
        } label: {
          if isInspecting {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Inspect", systemImage: "doc.text.magnifyingglass")
          }
        }
        .disabled(isBusy)

        Button {
          Task { _ = await onVerify() }
        } label: {
          if isVerifying {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Verify Disk", systemImage: "checkmark.shield")
          }
        }
        .disabled(isBusy)

        Button {
          isCompactConfirmationPresented = true
        } label: {
          if isCompacting {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Compact Disk", systemImage: "externaldrive.badge.minus")
          }
        }
        .buttonStyle(.borderedProminent)
        .disabled(isBusy)
      }

      Text(
        "Prepare primary-disk metadata, explicitly run disk creation when needed, inspect qemu-img info, then verify or compact the active disk through daemon boundaries."
      )
      .font(.callout)
      .foregroundStyle(.secondary)
      .fixedSize(horizontal: false, vertical: true)

      if let preparationError {
        ErrorLabel(message: preparationError)
      }

      if let preparation {
        DiskPreparationSummary(preparation: preparation)
      }

      if let creationError {
        ErrorLabel(message: creationError)
      }

      if let creation {
        DiskCreationSummary(creation: creation)
      }

      if let inspectionError {
        ErrorLabel(message: inspectionError)
      }

      if let inspection {
        DiskInspectionSummary(inspection: inspection)
      }

      if let verificationError {
        ErrorLabel(message: verificationError)
      }

      if let verification {
        DiskVerificationSummary(verification: verification)
      }

      if let compactionError {
        ErrorLabel(message: compactionError)
      }

      if let compaction {
        DiskCompactionSummary(compaction: compaction)
      }
    }
    .padding(12)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
    .alert("Create primary disk?", isPresented: $isCreateConfirmationPresented) {
      Button("Cancel", role: .cancel) {}
      Button("Create Disk") {
        Task { _ = await onCreate() }
      }
    } message: {
      Text(
        "Run the daemon disk creation step for this VM. Existing disk metadata may be reused if the active disk is already prepared."
      )
    }
    .alert("Compact active disk?", isPresented: $isCompactConfirmationPresented) {
      Button("Cancel", role: .cancel) {}
      Button("Compact", role: .destructive) {
        Task { _ = await onCompact() }
      }
    } message: {
      Text(
        "Compact the active disk now. This can take a while; the daemon keeps a backup of the previous image."
      )
    }
  }

  private var isBusy: Bool {
    isPreparing || isCreating || isInspecting || isVerifying || isCompacting
  }
}

private struct DiskPreparationSummary: View {
  var preparation: DiskPreparation

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Label("Primary disk metadata prepared.", systemImage: "checkmark.circle")
        .font(.callout)
        .foregroundStyle(.secondary)
      GuestToolsFactRow(title: "Path", value: preparation.path)
      GuestToolsFactRow(title: "Format", value: preparation.format)
      GuestToolsFactRow(title: "Size", value: preparation.size)
      GuestToolsFactRow(title: "Ready", value: preparation.exists ? "true" : "false")
      GuestToolsFactRow(title: "Created", value: preparation.created ? "true" : "false")
      GuestToolsFactRow(title: "Command", value: preparation.createCommandLine ?? "None")
      GuestToolsFactRow(title: "Prepared", value: unixTimeText(preparation.preparedAtUnix))
    }
    .padding(10)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct DiskCreationSummary: View {
  var creation: VMDiskCreation

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Label("Primary disk creation boundary completed.", systemImage: "checkmark.circle")
        .font(.callout)
        .foregroundStyle(.secondary)
      GuestToolsFactRow(title: "Executed", value: creation.executed ? "true" : "false")
      GuestToolsFactRow(title: "Status", value: creation.exitStatus ?? "None")
      GuestToolsFactRow(title: "Command", value: creation.commandLine)
      GuestToolsFactRow(title: "Path", value: creation.preparation.path)
      GuestToolsFactRow(title: "Ready", value: creation.preparation.exists ? "true" : "false")
      GuestToolsFactRow(title: "Created", value: unixTimeText(creation.createdAtUnix))
      if !creation.stderr.isEmpty {
        Text(creation.stderr)
          .font(.caption.monospaced())
          .foregroundStyle(.secondary)
          .lineLimit(6)
      }
    }
    .padding(10)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct DiskInspectionSummary: View {
  var inspection: VMDiskInspection

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Label("Primary disk inspected.", systemImage: "checkmark.circle")
        .font(.callout)
        .foregroundStyle(.secondary)
      GuestToolsFactRow(title: "Path", value: inspection.preparation.path)
      GuestToolsFactRow(title: "Status", value: inspection.exitStatus)
      GuestToolsFactRow(title: "Command", value: inspection.commandLine)
      GuestToolsFactRow(
        title: "Duration",
        value: "\(inspection.inspectDurationMicroseconds) microseconds"
      )
      GuestToolsFactRow(title: "Inspected", value: unixTimeText(inspection.inspectedAtUnix))
      Text(inspection.info)
        .font(.caption.monospaced())
        .foregroundStyle(.secondary)
        .lineLimit(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(8)
        .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
    }
    .padding(10)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct DiskVerificationSummary: View {
  var verification: VMDiskVerification

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Label("Active disk verification completed.", systemImage: "checkmark.circle")
        .font(.callout)
        .foregroundStyle(.secondary)
      GuestToolsFactRow(title: "Active disk", value: verification.activeDisk.path)
      GuestToolsFactRow(title: "Source", value: verification.activeDisk.sourceTitle)
      GuestToolsFactRow(title: "Status", value: verification.exitStatus)
      GuestToolsFactRow(title: "Command", value: verification.commandLine)
      GuestToolsFactRow(
        title: "Duration",
        value: "\(verification.verifyDurationMicroseconds) microseconds"
      )
      GuestToolsFactRow(title: "Verified", value: unixTimeText(verification.verifiedAtUnix))

      Text(verification.report)
        .font(.caption.monospaced())
        .foregroundStyle(.secondary)
        .lineLimit(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(8)
        .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
    }
    .padding(10)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct DiskCompactionSummary: View {
  var compaction: VMDiskCompaction

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Label("Active disk compacted with backup retained.", systemImage: "checkmark.circle")
        .font(.callout)
        .foregroundStyle(.secondary)
      GuestToolsFactRow(title: "Active disk", value: compaction.activeDisk.path)
      GuestToolsFactRow(title: "Backup", value: compaction.backupPath)
      GuestToolsFactRow(title: "Temp", value: compaction.tempPath)
      GuestToolsFactRow(title: "Status", value: compaction.exitStatus)
      GuestToolsFactRow(title: "Command", value: compaction.commandLine)
      GuestToolsFactRow(title: "Original", value: byteText(compaction.originalSizeBytes))
      GuestToolsFactRow(title: "Compacted", value: byteText(compaction.compactedSizeBytes))
      GuestToolsFactRow(title: "Saved", value: byteText(compaction.savedBytes))
      GuestToolsFactRow(
        title: "Duration",
        value: "\(compaction.compactDurationMicroseconds) microseconds"
      )
      GuestToolsFactRow(title: "Compacted", value: unixTimeText(compaction.compactedAtUnix))
    }
    .padding(10)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private func byteText(_ value: UInt64) -> String {
    ByteCountFormatter.string(fromByteCount: Int64(value), countStyle: .file)
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct MetadataRepairPanel: View {
  var repair: VMMetadataRepair?
  var isRepairing: Bool
  var error: String?
  var migration: VMManifestMigration?
  var isCheckingMigration: Bool
  var migrationError: String?
  var onRepair: () async -> Bool
  var onCheckMigration: () async -> Bool

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Text("Metadata Maintenance")
          .font(.headline)

        Spacer()

        Button {
          Task { _ = await onCheckMigration() }
        } label: {
          if isCheckingMigration {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Check Migration", systemImage: "doc.text.magnifyingglass")
          }
        }
        .disabled(isBusy)

        Button {
          Task { _ = await onRepair() }
        } label: {
          if isRepairing {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Repair Metadata", systemImage: "wrench.and.screwdriver")
          }
        }
        .buttonStyle(.borderedProminent)
        .disabled(isBusy)
      }

      Text(
        "Check manifest migration runs a dry-run schema validation only. Repair Metadata rebuilds repairable bundle metadata from existing manifests and snapshot records. Neither action creates disks, replaces corrupt JSON, or invents unrecoverable state."
      )
      .font(.callout)
      .foregroundStyle(.secondary)
      .fixedSize(horizontal: false, vertical: true)

      if let migrationError {
        ErrorLabel(message: migrationError)
      }

      if let migration {
        ManifestMigrationSummary(migration: migration)
      }

      if let error {
        ErrorLabel(message: error)
      }

      if let repair {
        MetadataRepairSummary(repair: repair)
      }
    }
    .padding(12)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private var isBusy: Bool {
    isRepairing || isCheckingMigration
  }
}

private struct ManifestMigrationSummary: View {
  var migration: VMManifestMigration

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Label(
        migration.migrated ? "Manifest migration available." : "Manifest schema is current.",
        systemImage: migration.migrated ? "arrow.triangle.2.circlepath" : "checkmark.seal"
      )
      .font(.callout)
      .foregroundStyle(.secondary)

      GuestToolsFactRow(title: "Mode", value: migration.dryRun ? "Dry run" : "Apply")
      GuestToolsFactRow(title: "Manifest", value: migration.manifestPath)
      GuestToolsFactRow(title: "Schema", value: "\(migration.fromSchema) -> \(migration.toSchema)")
      GuestToolsFactRow(title: "Migrated", value: migration.migrated ? "true" : "false")
      GuestToolsFactRow(title: "Backup", value: migration.backupPath ?? "None")
      GuestToolsFactRow(title: "Receipt", value: migration.receiptPath ?? "None")
      GuestToolsFactRow(title: "Timestamp", value: unixTimeText(migration.migratedAtUnix))

      if migration.actions.isEmpty {
        Text("No manifest migration action was reported.")
          .font(.caption)
          .foregroundStyle(.secondary)
      } else {
        VStack(alignment: .leading, spacing: 6) {
          ForEach(Array(migration.actions.enumerated()), id: \.offset) { _, action in
            Text(action)
              .font(.caption)
              .foregroundStyle(.secondary)
              .frame(maxWidth: .infinity, alignment: .leading)
              .padding(8)
              .background(
                Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
          }
        }
      }
    }
    .padding(10)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct MetadataRepairSummary: View {
  var repair: VMMetadataRepair

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Label(
        repair.repaired ? "Metadata repair completed." : "No metadata repairs needed.",
        systemImage: repair.repaired ? "checkmark.circle" : "checkmark.seal"
      )
      .font(.callout)
      .foregroundStyle(.secondary)

      GuestToolsFactRow(title: "Bundle", value: repair.bundle)
      GuestToolsFactRow(title: "Repaired", value: repair.repaired ? "true" : "false")
      GuestToolsFactRow(title: "Actions", value: "\(repair.actions.count)")
      GuestToolsFactRow(title: "Timestamp", value: unixTimeText(repair.repairedAtUnix))

      if repair.actions.isEmpty {
        Text("No metadata action was required.")
          .font(.caption)
          .foregroundStyle(.secondary)
      } else {
        VStack(alignment: .leading, spacing: 6) {
          ForEach(repair.actions) { action in
            VStack(alignment: .leading, spacing: 4) {
              Text(action.action)
                .font(.caption.weight(.semibold))
              Text(action.path)
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
              Text(action.detail)
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(8)
            .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
          }
        }
      }
    }
    .padding(10)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct PortableBundlePanel: View {
  var export: VMExportMetadata?
  var isExporting: Bool
  var exportError: String?
  var lastImport: VMImportMetadata?
  var isImporting: Bool
  var importError: String?
  var onExport: (String) async -> Bool
  var onImport: (String, String) async -> Bool

  @State private var exportOutput = ""
  @State private var importInput = ""
  @State private var importName = ""

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      Text("Portable Bundle")
        .font(.headline)

      Text(
        "Export or import portable VM bundles through the daemon file-copy boundary. This does not start a VM, connect QMP, attach guest tools, or migrate live guest state."
      )
      .font(.callout)
      .foregroundStyle(.secondary)
      .fixedSize(horizontal: false, vertical: true)

      VStack(alignment: .leading, spacing: 10) {
        HStack(spacing: 10) {
          TextField("Export output path", text: $exportOutput)
            .textFieldStyle(.roundedBorder)

          PathPickerButton(
            title: "Choose Export Destination",
            mode: .saveFile(defaultName: "export.vmbridge"),
            path: $exportOutput
          )

          Button {
            Task { _ = await onExport(exportOutput) }
          } label: {
            if isExporting {
              ProgressView()
                .controlSize(.small)
            } else {
              Label("Export", systemImage: "square.and.arrow.up")
            }
          }
          .disabled(
            isExporting || exportOutput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }

        if let exportError {
          ErrorLabel(message: exportError)
        }

        if let export {
          VMExportSummary(export: export)
        }
      }

      Divider()

      VStack(alignment: .leading, spacing: 10) {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 180), spacing: 10)], spacing: 10) {
          HStack(spacing: 10) {
            TextField("Import input path", text: $importInput)
              .textFieldStyle(.roundedBorder)

            PathPickerButton(
              title: "Choose Import Bundle", mode: .fileOrDirectory, path: $importInput)
          }
          TextField("Optional new name", text: $importName)
            .textFieldStyle(.roundedBorder)
        }

        HStack {
          Spacer()
          Button {
            Task { _ = await onImport(importInput, importName) }
          } label: {
            if isImporting {
              ProgressView()
                .controlSize(.small)
            } else {
              Label("Import", systemImage: "square.and.arrow.down")
            }
          }
          .buttonStyle(.borderedProminent)
          .disabled(
            isImporting || importInput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }

        if let importError {
          ErrorLabel(message: importError)
        }

        if let lastImport {
          VMImportSummary(imported: lastImport)
        }
      }
    }
    .padding(12)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }
}

private struct VMExportSummary: View {
  var export: VMExportMetadata

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Label("VM bundle exported.", systemImage: "checkmark.circle")
        .font(.callout)
        .foregroundStyle(.secondary)
      GuestToolsFactRow(title: "VM", value: export.vm)
      GuestToolsFactRow(title: "Source", value: export.source)
      GuestToolsFactRow(title: "Output", value: export.output)
      GuestToolsFactRow(title: "Archive format", value: export.archiveFormat)
      GuestToolsFactRow(title: "Copied files", value: copiedFileCountText(export.copiedFileCount))
      GuestToolsFactRow(title: "Manifest preserved", value: boolText(export.manifestPreserved))
      GuestToolsFactRow(title: "Metadata preserved", value: boolText(export.metadataPreserved))
      CopiedFilesList(files: export.copiedFiles)
      GuestToolsFactRow(title: "Exported", value: unixTimeText(export.exportedAtUnix))
    }
    .padding(10)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }

  private func copiedFileCountText(_ count: UInt64) -> String {
    count == 1 ? "1 copied bundle file" : "\(count) copied bundle files"
  }

  private func boolText(_ value: Bool) -> String {
    value ? "true" : "false"
  }
}

private struct VMImportSummary: View {
  var imported: VMImportMetadata

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Label("VM bundle imported.", systemImage: "checkmark.circle")
        .font(.callout)
        .foregroundStyle(.secondary)
      GuestToolsFactRow(title: "VM", value: imported.vm)
      GuestToolsFactRow(title: "Source", value: imported.source)
      GuestToolsFactRow(title: "Output", value: imported.output)
      GuestToolsFactRow(title: "Archive format", value: imported.archiveFormat)
      GuestToolsFactRow(title: "Copied files", value: copiedFileCountText(imported.copiedFileCount))
      GuestToolsFactRow(title: "Manifest preserved", value: boolText(imported.manifestPreserved))
      GuestToolsFactRow(title: "Metadata preserved", value: boolText(imported.metadataPreserved))
      GuestToolsFactRow(title: "Original name", value: imported.originalName)
      GuestToolsFactRow(title: "Requested name", value: imported.requestedName ?? "None")
      GuestToolsFactRow(
        title: "Manifest identity rewritten",
        value: boolText(imported.manifestIdentityRewritten)
      )
      CopiedFilesList(files: imported.copiedFiles)
      GuestToolsFactRow(title: "Imported", value: unixTimeText(imported.importedAtUnix))
    }
    .padding(10)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }

  private func copiedFileCountText(_ count: UInt64) -> String {
    count == 1 ? "1 copied bundle file" : "\(count) copied bundle files"
  }

  private func boolText(_ value: Bool) -> String {
    value ? "true" : "false"
  }
}

private struct CopiedFilesList: View {
  var files: [String]

  var body: some View {
    if !files.isEmpty {
      VStack(alignment: .leading, spacing: 4) {
        Text("Copied file paths")
          .font(.caption)
          .foregroundStyle(.secondary)
        ForEach(files, id: \.self) { file in
          Text(file)
            .font(.caption.monospaced())
            .foregroundStyle(.secondary)
            .textSelection(.enabled)
        }
      }
    }
  }
}

private struct DiagnosticsPerformancePanel: View {
  var diagnosticBundle: DiagnosticBundle?
  var isCreatingDiagnosticBundle: Bool
  var diagnosticBundleError: String?
  var performanceBaseline: PerformanceBaseline?
  var isCreatingPerformanceBaseline: Bool
  var performanceBaselineError: String?
  var performanceSample: PerformanceSample?
  var isCreatingPerformanceSample: Bool
  var performanceSampleError: String?
  var onCreateDiagnosticBundle: (String) async -> Bool
  var onCreatePerformanceBaseline: (String) async -> Bool
  var onCreatePerformanceSample: (String, String, String, Bool) async -> Bool

  @State private var diagnosticsOutput = ""
  @State private var baselineOutput = ""
  @State private var sampleOutput = ""
  @State private var sampleArtifactBytes = "4096"
  @State private var sampleIterations = "1"
  @State private var sampleSync = false

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Text("Diagnostics & Performance")
          .font(.headline)

        Spacer()
      }

      Text(
        "Create redacted support bundles and bounded host-side performance metadata without starting guest workloads."
      )
      .font(.callout)
      .foregroundStyle(.secondary)
      .fixedSize(horizontal: false, vertical: true)

      VStack(alignment: .leading, spacing: 10) {
        HStack(spacing: 10) {
          TextField("Diagnostics output directory", text: $diagnosticsOutput)
            .textFieldStyle(.roundedBorder)

          PathPickerButton(
            title: "Choose Diagnostics Directory",
            mode: .directory,
            path: $diagnosticsOutput
          )

          Button {
            Task { _ = await onCreateDiagnosticBundle(diagnosticsOutput) }
          } label: {
            if isCreatingDiagnosticBundle {
              ProgressView()
                .controlSize(.small)
            } else {
              Label("Bundle", systemImage: "shippingbox")
            }
          }
          .disabled(
            isCreatingDiagnosticBundle
              || diagnosticsOutput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
          )
        }

        if let diagnosticBundleError {
          ErrorLabel(message: diagnosticBundleError)
        }

        if let diagnosticBundle {
          DiagnosticBundleSummary(bundle: diagnosticBundle)
        }
      }

      Divider()

      VStack(alignment: .leading, spacing: 10) {
        HStack(spacing: 10) {
          TextField("Baseline output directory", text: $baselineOutput)
            .textFieldStyle(.roundedBorder)

          PathPickerButton(
            title: "Choose Baseline Directory",
            mode: .directory,
            path: $baselineOutput
          )

          Button {
            Task { _ = await onCreatePerformanceBaseline(baselineOutput) }
          } label: {
            if isCreatingPerformanceBaseline {
              ProgressView()
                .controlSize(.small)
            } else {
              Label("Baseline", systemImage: "gauge.with.dots.needle.33percent")
            }
          }
          .disabled(
            isCreatingPerformanceBaseline
              || baselineOutput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
          )
        }

        if let performanceBaselineError {
          ErrorLabel(message: performanceBaselineError)
        }

        if let performanceBaseline {
          PerformanceBaselineSummary(baseline: performanceBaseline)
        }
      }

      Divider()

      VStack(alignment: .leading, spacing: 10) {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 160), spacing: 10)], spacing: 10) {
          HStack(spacing: 10) {
            TextField("Sample output directory", text: $sampleOutput)
              .textFieldStyle(.roundedBorder)

            PathPickerButton(
              title: "Choose Sample Directory",
              mode: .directory,
              path: $sampleOutput
            )
          }
          TextField("Probe bytes", text: $sampleArtifactBytes)
            .textFieldStyle(.roundedBorder)
          TextField("Iterations", text: $sampleIterations)
            .textFieldStyle(.roundedBorder)
          Toggle("Sync writes", isOn: $sampleSync)
        }

        HStack {
          Spacer()
          Button {
            Task {
              _ = await onCreatePerformanceSample(
                sampleOutput,
                sampleArtifactBytes,
                sampleIterations,
                sampleSync
              )
            }
          } label: {
            if isCreatingPerformanceSample {
              ProgressView()
                .controlSize(.small)
            } else {
              Label("Sample", systemImage: "waveform.path.ecg")
            }
          }
          .buttonStyle(.borderedProminent)
          .disabled(
            isCreatingPerformanceSample
              || sampleOutput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
          )
        }

        if let performanceSampleError {
          ErrorLabel(message: performanceSampleError)
        }

        if let performanceSample {
          PerformanceSampleSummary(sample: performanceSample)
        }
      }
    }
    .padding(12)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }
}

private struct DiagnosticBundleSummary: View {
  var bundle: DiagnosticBundle

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Label("Redacted diagnostic bundle created.", systemImage: "checkmark.circle")
        .font(.callout)
        .foregroundStyle(.secondary)
      GuestToolsFactRow(title: "VM", value: bundle.vm)
      GuestToolsFactRow(title: "Output", value: bundle.output)
      GuestToolsFactRow(title: "Source", value: bundle.source)
      GuestToolsFactRow(title: "Files", value: bundle.fileCountTitle)
      GuestToolsFactRow(title: "File list", value: bundle.fileListTitle)
      GuestToolsFactRow(title: "Created", value: unixTimeText(bundle.createdAtUnix))
    }
    .padding(10)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct PerformanceBaselineSummary: View {
  var baseline: PerformanceBaseline

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Label("Metadata-only baseline created.", systemImage: "checkmark.circle")
        .font(.callout)
        .foregroundStyle(.secondary)
      GuestToolsFactRow(title: "VM", value: baseline.vm)
      GuestToolsFactRow(title: "Output", value: baseline.output)
      GuestToolsFactRow(title: "Artifact", value: baseline.artifact)
      GuestToolsFactRow(title: "Source", value: baseline.source)
      GuestToolsFactRow(title: "State", value: baseline.state.title)
      GuestToolsFactRow(title: "Metadata only", value: baseline.metadataOnly ? "true" : "false")
      if let runner = baseline.runner {
        GuestToolsFactRow(title: "Runner", value: runner.engine)
      }
      if let metrics = baseline.metrics {
        GuestToolsFactRow(title: "CPU", value: "\(metrics.cpuPercent)%")
        GuestToolsFactRow(title: "Memory", value: "\(metrics.memoryUsedMiB) MiB")
      }
      GuestToolsFactRow(title: "Created", value: unixTimeText(baseline.createdAtUnix))
      PerformanceMeasurementList(measurements: baseline.measurements)
      PerformanceNotesList(notes: baseline.notes)
    }
    .padding(10)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct PerformanceSampleSummary: View {
  var sample: PerformanceSample

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Label("Host-side performance sample created.", systemImage: "checkmark.circle")
        .font(.callout)
        .foregroundStyle(.secondary)
      GuestToolsFactRow(title: "VM", value: sample.vm)
      GuestToolsFactRow(title: "Output", value: sample.output)
      GuestToolsFactRow(title: "Artifact", value: sample.artifact)
      GuestToolsFactRow(title: "Source", value: sample.source)
      GuestToolsFactRow(title: "Probe", value: sample.probe)
      GuestToolsFactRow(
        title: "Probe files", value: sample.probes.isEmpty ? "None" : "\(sample.probes.count)")
      GuestToolsFactRow(title: "Probe bytes", value: String(sample.artifactBytes))
      GuestToolsFactRow(title: "Iterations", value: String(sample.iterations))
      GuestToolsFactRow(title: "Sync", value: sample.sync ? "true" : "false")
      GuestToolsFactRow(title: "State", value: sample.state.title)
      if let runner = sample.runner {
        GuestToolsFactRow(title: "Runner", value: runner.engine)
      }
      if let metrics = sample.metrics {
        GuestToolsFactRow(title: "CPU", value: "\(metrics.cpuPercent)%")
        GuestToolsFactRow(title: "Memory", value: "\(metrics.memoryUsedMiB) MiB")
      }
      GuestToolsFactRow(title: "Created", value: unixTimeText(sample.createdAtUnix))
      PerformanceSampleIterationList(iterations: sample.iterationResults)
      PerformanceProbeList(probes: sample.probes)
      PerformanceMeasurementList(measurements: sample.measurements)
      PerformanceNotesList(notes: sample.notes)
    }
    .padding(10)
    .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct PerformanceMeasurementList: View {
  var measurements: [PerformanceMeasurement]

  var body: some View {
    if measurements.isEmpty {
      Text("No measurements reported.")
        .font(.callout)
        .foregroundStyle(.secondary)
    } else {
      VStack(alignment: .leading, spacing: 6) {
        ForEach(measurements) { measurement in
          HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 2) {
              Text(measurement.name)
                .font(.caption.weight(.medium))
              Text(measurement.source)
                .font(.caption2)
                .foregroundStyle(.secondary)
            }

            Spacer()

            Text(measurement.valueTitle)
              .font(.caption.monospacedDigit())
              .foregroundStyle(measurement.metadataOnly ? .secondary : .primary)
          }
        }
      }
    }
  }
}

private struct PerformanceSampleIterationList: View {
  var iterations: [PerformanceSampleIteration]

  var body: some View {
    if iterations.isEmpty {
      Text("No sample iterations reported.")
        .font(.callout)
        .foregroundStyle(.secondary)
    } else {
      VStack(alignment: .leading, spacing: 6) {
        Text("Sample iterations")
          .font(.caption.weight(.semibold))
        ForEach(iterations) { iteration in
          HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 2) {
              Text("Iteration \(iteration.iteration)")
                .font(.caption.weight(.medium))
              Text(iteration.probe)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(1)
            }

            Spacer()

            VStack(alignment: .trailing, spacing: 2) {
              Text(iteration.writeLatencyTitle)
                .font(.caption.monospacedDigit())
              Text(iteration.bytesTitle)
                .font(.caption2.monospacedDigit())
                .foregroundStyle(.secondary)
            }
          }
        }
      }
    }
  }
}

private struct PerformanceProbeList: View {
  var probes: [String]

  var body: some View {
    if !probes.isEmpty {
      VStack(alignment: .leading, spacing: 4) {
        Text("Probe files")
          .font(.caption.weight(.semibold))
        ForEach(probes, id: \.self) { probe in
          Text(probe)
            .font(.caption.monospaced())
            .foregroundStyle(.secondary)
            .lineLimit(1)
        }
      }
    }
  }
}

private struct PerformanceNotesList: View {
  var notes: [String]

  var body: some View {
    if !notes.isEmpty {
      VStack(alignment: .leading, spacing: 4) {
        ForEach(notes, id: \.self) { note in
          Text(note)
            .font(.caption)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        }
      }
    }
  }
}

private struct ErrorLabel: View {
  var message: String

  var body: some View {
    Label(message, systemImage: "exclamationmark.triangle")
      .font(.callout)
      .foregroundStyle(.red)
      .textSelection(.enabled)
  }
}

private struct LogViewerPanel: View {
  var qemuLog: VMLogView?
  var serialLog: VMLogView?
  var isLoading: Bool
  var error: String?
  var onLoad: (VMLogKind) async -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Text("Logs")
          .font(.headline)

        Spacer()

        Button {
          Task { await onLoad(.qemu) }
        } label: {
          if isLoading {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("QEMU", systemImage: "terminal")
          }
        }
        .disabled(isLoading)

        Button {
          Task { await onLoad(.serial) }
        } label: {
          Label("Serial", systemImage: "text.alignleft")
        }
        .disabled(isLoading)
      }

      if let error {
        Label(error, systemImage: "exclamationmark.triangle")
          .font(.callout)
          .foregroundStyle(.red)
      }

      if let qemuLog {
        LogTailView(log: qemuLog)
      }

      if let serialLog {
        LogTailView(log: serialLog)
      }

      if qemuLog == nil && serialLog == nil && error == nil {
        Text("Load the QEMU or serial log tail from the daemon-backed VM bundle.")
          .font(.callout)
          .foregroundStyle(.secondary)
      }
    }
    .padding(12)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }
}

private struct LogTailView: View {
  var log: VMLogView

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      HStack {
        Label(log.kind.title, systemImage: log.exists ? "doc.text" : "doc.badge.plus")
          .font(.subheadline.weight(.semibold))
        Spacer()
        Text(byteSummary)
          .font(.caption)
          .foregroundStyle(.secondary)
      }

      Text(log.path)
        .font(.caption)
        .foregroundStyle(.secondary)
        .lineLimit(1)
        .truncationMode(.middle)

      if log.exists {
        ScrollView {
          Text(log.content.isEmpty ? "Log file is empty." : log.content)
            .font(.system(.caption, design: .monospaced))
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(8)
        }
        .frame(maxHeight: 180)
        .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
      } else {
        Label("Log file has not been written yet.", systemImage: "info.circle")
          .font(.callout)
          .foregroundStyle(.secondary)
      }
    }
  }

  private var byteSummary: String {
    let suffix = log.truncated ? " tail" : ""
    return "\(log.returnedBytes)/\(log.bytes) bytes\(suffix)"
  }
}

private struct ReadinessBlockerList: View {
  var readiness: LaunchReadiness

  var body: some View {
    if readiness.blockers.isEmpty {
      Label("No launch blockers reported.", systemImage: "checkmark.circle")
        .font(.callout)
        .foregroundStyle(.secondary)
    } else {
      VStack(alignment: .leading, spacing: 8) {
        ForEach(readiness.blockers) { blocker in
          VStack(alignment: .leading, spacing: 3) {
            Text(blocker.code)
              .font(.caption.weight(.medium))
            Text(blocker.message)
              .font(.caption)
              .foregroundStyle(.secondary)
              .fixedSize(horizontal: false, vertical: true)
            if let path = blocker.path {
              Text(path)
                .font(.caption2.monospaced())
                .foregroundStyle(.secondary)
                .lineLimit(2)
            }
            if let capability = blocker.capability {
              Text("Capability: \(capability)")
                .font(.caption2.monospaced())
                .foregroundStyle(.secondary)
                .lineLimit(2)
            }
          }
          .frame(maxWidth: .infinity, alignment: .leading)
          .padding(8)
          .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
        }
      }
    }
  }
}

private struct SharedFolderManifestPanel: View {
  var list: VMSharedFolderList?
  var isLoading: Bool
  var isAdding: Bool
  var isRemoving: Bool
  var error: String?
  var onRefresh: () async -> Void
  var onAdd: (String, String, Bool, String) async -> Bool
  var onRemove: (String) async -> Bool

  @State private var name = ""
  @State private var hostPath = ""
  @State private var hostPathToken = ""
  @State private var readOnly = false

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Text("Shared Folder Manifest")
          .font(.headline)

        Spacer()

        Button {
          Task { await onRefresh() }
        } label: {
          if isLoading {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Refresh", systemImage: "arrow.clockwise")
          }
        }
        .disabled(isLoading || isAdding || isRemoving)
      }

      Text(
        "Manage approved host folders recorded in the VM manifest. This changes policy metadata only; use Guest Tools mount actions to request a live guest mount."
      )
      .font(.callout)
      .foregroundStyle(.secondary)
      .fixedSize(horizontal: false, vertical: true)

      if let error {
        ErrorLabel(message: error)
      }

      if let list {
        SharedFolderRows(
          folders: list.sharedFolders,
          isRemoving: isRemoving,
          onRemove: onRemove
        )
      } else {
        Text("Refresh to inspect approved shared folders.")
          .font(.callout)
          .foregroundStyle(.secondary)
      }

      Divider()

      VStack(alignment: .leading, spacing: 8) {
        Text("Add Approval")
          .font(.callout)
          .foregroundStyle(.secondary)

        HStack(spacing: 8) {
          TextField("Name", text: $name)
            .textFieldStyle(.roundedBorder)
            .frame(minWidth: 120)
          HStack(spacing: 8) {
            TextField("Host path", text: $hostPath)
              .textFieldStyle(.roundedBorder)
              .frame(minWidth: 180)

            PathPickerButton(title: "Choose Host Folder", mode: .directory, path: $hostPath)
          }
        }

        HStack(spacing: 8) {
          SecureField("Host path token", text: $hostPathToken)
            .textFieldStyle(.roundedBorder)
          Toggle("Read-only", isOn: $readOnly)
            .toggleStyle(.checkbox)

          Button {
            Task {
              let added = await onAdd(name, hostPath, readOnly, hostPathToken)
              if added {
                name = ""
                hostPath = ""
                hostPathToken = ""
                readOnly = false
              }
            }
          } label: {
            if isAdding {
              ProgressView()
                .controlSize(.small)
            } else {
              Label("Add", systemImage: "folder.badge.plus")
            }
          }
          .buttonStyle(.borderedProminent)
          .disabled(isLoading || isAdding || isRemoving)
        }
      }
    }
    .padding(12)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }
}

private struct SharedFolderRows: View {
  var folders: [VMSharedFolder]
  var isRemoving: Bool
  var onRemove: (String) async -> Bool
  @State private var removeConfirmationFolderName = ""
  @State private var isRemoveConfirmationPresented = false

  var body: some View {
    if folders.isEmpty {
      Text("No shared folders approved")
        .font(.callout)
        .foregroundStyle(.secondary)
    } else {
      VStack(alignment: .leading, spacing: 8) {
        ForEach(folders) { folder in
          HStack(alignment: .center, spacing: 10) {
            VStack(alignment: .leading, spacing: 3) {
              HStack(spacing: 6) {
                Text(folder.name)
                  .font(.callout.weight(.medium))
                Text(folder.readOnly ? "read-only" : "read-write")
                  .font(.caption)
                  .foregroundStyle(.secondary)
              }
              Text(sharedFolderHostPathDisplay(folder.hostPath))
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
              Text(sharedFolderTokenDisplay(folder.hostPathToken))
                .font(.caption2.monospaced())
                .foregroundStyle(.secondary)
                .lineLimit(1)
            }

            Spacer()

            Button(role: .destructive) {
              removeConfirmationFolderName = folder.name
              isRemoveConfirmationPresented = true
            } label: {
              Label("Remove", systemImage: "minus.circle")
            }
            .controlSize(.small)
            .disabled(isRemoving)
          }
          .padding(8)
          .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
        }
      }
      .alert("Remove shared folder?", isPresented: $isRemoveConfirmationPresented) {
        Button("Cancel", role: .cancel) {}
        Button("Remove", role: .destructive) {
          let folderName = removeConfirmationFolderName
          removeConfirmationFolderName = ""
          Task { _ = await onRemove(folderName) }
        }
      } message: {
        Text("Remove \(removeConfirmationFolderName) from the approved shared-folder metadata.")
      }
    }
  }
}

private func sharedFolderHostPathDisplay(_ hostPath: String) -> String {
  let path = hostPath.trimmingCharacters(in: .whitespacesAndNewlines)
  guard !path.isEmpty else {
    return "Host folder hidden"
  }

  let basename = URL(fileURLWithPath: path).lastPathComponent
  guard !basename.isEmpty else {
    return "Host folder hidden"
  }

  return "Host folder: \(basename)"
}

private func sharedFolderTokenDisplay(_ token: String) -> String {
  let trimmed = token.trimmingCharacters(in: .whitespacesAndNewlines)
  guard !trimmed.isEmpty else {
    return "Token hidden"
  }

  let suffix = String(trimmed.suffix(4))
  return "Token ending \(suffix)"
}

private struct GuestToolsStatusPanel: View {
  var status: GuestToolsStatus?
  var guestWindowProxyStatus: GuestWindowProxyStatus
  var provisioning: GuestToolsProvisioning?
  var isLoading: Bool
  var isSendingCommand: Bool
  var error: String?
  var provisioningError: String?
  var onRefresh: () async -> Void
  var onMountApprovedSharedFolder: (String) async -> Bool
  var onUnmountApprovedSharedFolder: (String) async -> Bool
  var onSendCommand: (GuestToolsAgentCommand) async -> Bool
  var onSyncGuestTime: () async -> Bool
  var onSetClipboardText: (String) async -> Bool
  var onResizeDisplay: (String, String, String) async -> Bool
  var onLaunchApplication: (String) async -> Bool
  var onFocusWindow: (String) async -> Bool
  var onCloseWindow: (String) async -> Bool
  var onOpenWindowProxy: (GuestToolsWindowAction) async -> Bool
  var onCloseWindowProxies: () -> Void
  var onSendInlineFileDrop: (String, String) async -> Bool

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Text("Guest Tools")
          .font(.headline)

        Spacer()

        if guestWindowProxyStatus.isTrackingWindows {
          Button {
            onCloseWindowProxies()
          } label: {
            Label("Close Proxies", systemImage: "xmark.rectangle")
          }
          .controlSize(.small)
        }

        Button {
          Task { await onRefresh() }
        } label: {
          if isLoading {
            ProgressView()
              .controlSize(.small)
          } else {
            Label("Refresh", systemImage: "arrow.clockwise")
          }
        }
        .disabled(isLoading)
      }

      if let error {
        Label(error, systemImage: "exclamationmark.triangle")
          .font(.callout)
          .foregroundStyle(.red)
      } else if let status {
        VStack(alignment: .leading, spacing: 10) {
          LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 14)], spacing: 14) {
            GuestToolsStatusBadge(
              title: "Policy",
              value: status.tools.capitalized,
              systemImage: "wrench.and.screwdriver"
            )
            GuestToolsStatusBadge(
              title: "Runtime",
              value: status.connected ? "Connected" : "Not connected",
              systemImage: status.connected ? "link" : "link.badge.plus"
            )
            GuestToolsStatusBadge(
              title: "Capabilities",
              value: "\(status.capabilities.count)",
              systemImage: "checklist"
            )
            GuestToolsStatusBadge(
              title: "Network",
              value: status.networkReadinessTitle,
              systemImage: status.primaryIPAddress == nil ? "network.slash" : "network"
            )
            GuestToolsStatusBadge(
              title: "Display",
              value: status.displayReadinessTitle,
              systemImage: "rectangle.inset.filled"
            )
            GuestToolsStatusBadge(
              title: "Clipboard",
              value: status.clipboardReadinessTitle,
              systemImage: "list.bullet.clipboard"
            )
            GuestToolsStatusBadge(
              title: "Shared Folders",
              value: status.sharedFoldersReadinessTitle,
              systemImage: "folder"
            )
            GuestToolsStatusBadge(
              title: "Window Proxy",
              value: guestWindowProxyStatus.badgeTitle,
              systemImage: guestWindowProxyStatus.isTrackingWindows
                ? "macwindow.on.rectangle" : "macwindow.badge.plus"
            )
            GuestToolsStatusBadge(
              title: "Host Approval",
              value: status.approvedSharedFoldersTitle,
              systemImage: status.approvedSharedFolders.isEmpty ? "lock.slash" : "lock.open"
            )
          }

          VStack(alignment: .leading, spacing: 6) {
            GuestToolsFactRow(title: "Tools Token", value: unixTimeText(status.tokenCreatedAtUnix))
            GuestToolsFactRow(title: "Window Proxy", value: guestWindowProxyStatus.detailText)
            if guestWindowProxyStatus.isTrackingWindows {
              GuestToolsFactRow(
                title: "Proxy Windows",
                value: guestWindowProxyStatus.windowSummaryText
              )
              GuestToolsFactRow(
                title: "Proxy Crop",
                value: guestWindowProxyStatus.cropFrameText
              )
            }
            GuestToolsFactRow(
              title: "Approved Shares",
              value: approvedSharedFolderText(status.approvedSharedFolders))
          }

          GuestToolsProvisioningView(
            provisioning: provisioning,
            error: provisioningError,
            fallbackTokenCreatedAtUnix: status.tokenCreatedAtUnix
          )

          ApprovedSharedFolderMountList(
            status: status,
            isLoading: isLoading,
            onMount: onMountApprovedSharedFolder,
            onUnmount: onUnmountApprovedSharedFolder
          )

          GuestToolsCommandActionList(
            status: status,
            isSending: isSendingCommand,
            onSend: onSendCommand,
            onSyncGuestTime: onSyncGuestTime,
            onSetClipboardText: onSetClipboardText,
            onResizeDisplay: onResizeDisplay,
            onLaunchApplication: onLaunchApplication,
            onFocusWindow: onFocusWindow,
            onCloseWindow: onCloseWindow,
            onSendInlineFileDrop: onSendInlineFileDrop
          )

          if let runtime = status.runtime {
            VStack(alignment: .leading, spacing: 6) {
              GuestToolsFactRow(title: "Guest OS", value: runtime.guestOS ?? "Unknown")
              GuestToolsFactRow(title: "Agent", value: runtime.agentVersion ?? "Unknown")
              GuestToolsFactRow(title: "IP", value: ipAddressText(runtime.guestIPAddresses))
              GuestToolsFactRow(
                title: "Heartbeat", value: unixTimeText(runtime.lastHeartbeatAtUnix))
              GuestToolsFactRow(title: "Shares", value: sharedFolderText(runtime.sharedFolders))
              if let metrics = runtime.metrics {
                GuestToolsFactRow(
                  title: "Metrics",
                  value: "\(metrics.cpuPercent)% CPU / \(metrics.memoryUsedMiB) MiB"
                )
              }
            }

            GuestClipboardTelemetryView(clipboard: runtime.lastClipboard)
            GuestToolsAgentUpdateView(update: runtime.agentUpdate)
            LastGuestToolsCommandResultView(
              result: runtime.lastCommandResult,
              isSending: isSendingCommand,
              onLaunchApplication: onLaunchApplication,
              onFocusWindow: onFocusWindow,
              onCloseWindow: onCloseWindow,
              onOpenWindowProxy: onOpenWindowProxy
            )
          }

          CapabilityList(capabilities: status.capabilities)
        }
      } else {
        Text("Refresh to inspect guest tools policy and runtime readiness.")
          .font(.callout)
          .foregroundStyle(.secondary)
      }
    }
    .padding(12)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }

  private func ipAddressText(_ addresses: [GuestToolsIPAddress]) -> String {
    guard !addresses.isEmpty else {
      return "Unavailable"
    }
    return
      addresses
      .map { address in
        if let interface = address.interface, !interface.isEmpty {
          return "\(address.address) (\(interface))"
        }
        return address.address
      }
      .joined(separator: ", ")
  }

  private func sharedFolderText(_ sharedFolders: [GuestToolsSharedFolder]) -> String {
    guard !sharedFolders.isEmpty else {
      return "No runtime entries"
    }
    return
      sharedFolders
      .map { "\($0.name) (\(sharedFolderTokenDisplay($0.hostPathToken)))" }
      .joined(separator: ", ")
  }

  private func approvedSharedFolderText(_ sharedFolders: [GuestToolsApprovedSharedFolder]) -> String
  {
    guard !sharedFolders.isEmpty else {
      return "No host approvals"
    }
    return
      sharedFolders
      .map { folder in
        let access = folder.readOnly ? "read-only" : "read-write"
        return
          "\(folder.name): \(sharedFolderHostPathDisplay(folder.hostPath)) (\(sharedFolderTokenDisplay(folder.hostPathToken)), \(folder.approval), \(access))"
      }
      .joined(separator: ", ")
  }

  private func unixTimeText(_ value: UInt64?) -> String {
    guard let value else {
      return "Unavailable"
    }
    return Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct ApprovedSharedFolderMountList: View {
  var status: GuestToolsStatus
  var isLoading: Bool
  var onMount: (String) async -> Bool
  var onUnmount: (String) async -> Bool

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      Text("Mount Actions")
        .font(.callout)
        .foregroundStyle(.secondary)

      if status.approvedSharedFolders.isEmpty {
        Text("No approved shares")
          .font(.callout)
          .foregroundStyle(.secondary)
      } else {
        ForEach(status.approvedSharedFolders) { folder in
          HStack(alignment: .center, spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
              Text(folder.name)
                .font(.callout.weight(.medium))
                .lineLimit(1)
              Text(status.mountReadinessTitle(for: folder))
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
            }

            Spacer()

            Button {
              Task { _ = await onMount(folder.name) }
            } label: {
              Label("Mount", systemImage: "externaldrive.badge.plus")
            }
            .controlSize(.small)
            .disabled(isLoading || !status.canMountApprovedSharedFolder(folder))

            Button {
              Task { _ = await onUnmount(folder.name) }
            } label: {
              Label("Unmount", systemImage: "externaldrive.badge.minus")
            }
            .controlSize(.small)
            .disabled(isLoading || !status.canUnmountApprovedSharedFolder(folder))
          }
          .font(.callout)
        }
      }
    }
  }
}

private struct GuestToolsProvisioningView: View {
  var provisioning: GuestToolsProvisioning?
  var error: String?
  var fallbackTokenCreatedAtUnix: UInt64

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Text("Provisioning")
        .font(.callout)
        .foregroundStyle(.secondary)

      if let error {
        Label(error, systemImage: "exclamationmark.triangle")
          .font(.caption)
          .foregroundStyle(.secondary)
      }

      VStack(alignment: .leading, spacing: 6) {
        GuestToolsFactRow(title: "Token", value: tokenText)
        GuestToolsFactRow(title: "Token Created", value: unixTimeText(tokenCreatedAtUnix))
      }

      if let deviceCommand = provisioning?.deviceCommand {
        GuestToolsLinuxCommandRow(command: deviceCommand)
      }

      if let socketCommand = provisioning?.socketCommand {
        GuestToolsLinuxCommandRow(command: socketCommand)
      }
    }
  }

  private var tokenText: String {
    guard let token = provisioning?.token else {
      return "Hidden"
    }

    return token.hasToken ? "Hidden (\(token.tokenLength) chars)" : "Unavailable"
  }

  private var tokenCreatedAtUnix: UInt64 {
    provisioning?.token?.createdAtUnix ?? fallbackTokenCreatedAtUnix
  }

  private func unixTimeText(_ value: UInt64?) -> String {
    guard let value else {
      return "Unavailable"
    }
    return Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct GuestToolsLinuxCommandRow: View {
  var command: GuestToolsLinuxCommand

  var body: some View {
    HStack(alignment: .firstTextBaseline, spacing: 8) {
      VStack(alignment: .leading, spacing: 2) {
        Text("\(command.transport.title) command")
          .font(.caption.weight(.medium))
        Text(command.commandLine)
          .font(.caption.monospaced())
          .foregroundStyle(.secondary)
          .lineLimit(2)
          .textSelection(.enabled)
      }

      Spacer()

      Button {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(command.commandLine, forType: .string)
      } label: {
        Label("Copy", systemImage: "doc.on.doc")
      }
      .controlSize(.small)
    }
  }
}

private struct GuestToolsCommandActionList: View {
  private static let maxInlineFileDropBytes = 64 * 1024

  var status: GuestToolsStatus
  var isSending: Bool
  var onSend: (GuestToolsAgentCommand) async -> Bool
  var onSyncGuestTime: () async -> Bool
  var onSetClipboardText: (String) async -> Bool
  var onResizeDisplay: (String, String, String) async -> Bool
  var onLaunchApplication: (String) async -> Bool
  var onFocusWindow: (String) async -> Bool
  var onCloseWindow: (String) async -> Bool
  var onSendInlineFileDrop: (String, String) async -> Bool

  @State private var clipboardText = ""
  @State private var displayWidth = "1440"
  @State private var displayHeight = "900"
  @State private var displayScale = "2"
  @State private var applicationID = ""
  @State private var windowID = ""
  @State private var dropFileName = "notes.txt"
  @State private var dropContents = ""
  @State private var pendingCloseWindowID = ""
  @State private var isCloseWindowConfirmationPresented = false

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Text("Agent-Acknowledged Commands")
        .font(.callout)
        .foregroundStyle(.secondary)

      Text(
        "Buttons are enabled only for capabilities reported by the connected agent; in-guest effects depend on a matching command acknowledgement."
      )
        .font(.caption)
        .foregroundStyle(.secondary)
        .fixedSize(horizontal: false, vertical: true)

      HStack(alignment: .center, spacing: 8) {
        TextField("Clipboard text", text: $clipboardText)
          .textFieldStyle(.roundedBorder)

        Button {
          Task {
            if await onSetClipboardText(clipboardText) {
              clipboardText = ""
            }
          }
        } label: {
          commandLabel("Set Clipboard", systemImage: "list.bullet.clipboard")
        }
        .disabled(
          isSending || !canSend(capability: "clipboard")
            || clipboardText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        )
      }
      .controlSize(.small)

      HStack(alignment: .center, spacing: 8) {
        TextField("Width", text: $displayWidth)
          .textFieldStyle(.roundedBorder)
          .frame(width: 72)
        TextField("Height", text: $displayHeight)
          .textFieldStyle(.roundedBorder)
          .frame(width: 72)
        TextField("Scale", text: $displayScale)
          .textFieldStyle(.roundedBorder)
          .frame(width: 60)

        Button {
          Task { _ = await onResizeDisplay(displayWidth, displayHeight, displayScale) }
        } label: {
          commandLabel("Resize Display", systemImage: "rectangle.resize")
        }
        .disabled(isSending || !canSubmitDisplayResize)
      }
      .controlSize(.small)

      HStack(spacing: 10) {
        Button {
          Task { _ = await onSyncGuestTime() }
        } label: {
          commandLabel("Sync Time", systemImage: "clock.arrow.circlepath")
        }
        .disabled(isSending || !canSend(capability: "time-sync"))

        Button {
          Task { _ = await onSend(.listApplications) }
        } label: {
          commandLabel("List Applications", systemImage: "app.badge")
        }
        .disabled(isSending || !canSend(capability: "applications"))

        Button {
          Task { _ = await onSend(.listWindows) }
        } label: {
          commandLabel("List Windows", systemImage: "macwindow")
        }
        .disabled(isSending || !canSend(capability: "windows"))

        Spacer()
      }
      .controlSize(.small)

      HStack(alignment: .center, spacing: 8) {
        TextField("Application ID", text: $applicationID)
          .textFieldStyle(.roundedBorder)

        Button {
          Task {
            if await onLaunchApplication(applicationID) {
              applicationID = ""
            }
          }
        } label: {
          commandLabel("Launch", systemImage: "play.circle")
        }
        .disabled(isSending || !canSubmitApplicationID)
      }
      .controlSize(.small)

      HStack(alignment: .center, spacing: 8) {
        TextField("Window ID", text: $windowID)
          .textFieldStyle(.roundedBorder)

        Button {
          Task { _ = await onFocusWindow(windowID) }
        } label: {
          commandLabel("Focus", systemImage: "scope")
        }
        .disabled(isSending || !canSubmitWindowID)

        Button {
          pendingCloseWindowID = windowID.trimmingCharacters(in: .whitespacesAndNewlines)
          isCloseWindowConfirmationPresented = true
        } label: {
          commandLabel("Close", systemImage: "xmark.circle")
        }
        .disabled(isSending || !canSubmitWindowID)
      }
      .controlSize(.small)

      HStack(alignment: .center, spacing: 8) {
        TextField("File name", text: $dropFileName)
          .textFieldStyle(.roundedBorder)
          .frame(minWidth: 120)
        TextField("Inline file contents", text: $dropContents)
          .textFieldStyle(.roundedBorder)

        Button {
          Task {
            if await onSendInlineFileDrop(dropFileName, dropContents) {
              dropContents = ""
            }
          }
        } label: {
          commandLabel("Drop Text", systemImage: "doc.badge.plus")
        }
        .help(inlineFileDropValidationMessage ?? "Queue inline text as a guest file-drop request.")
        .disabled(isSending || !canSubmitInlineFileDrop)
      }
      .controlSize(.small)

      if !status.connected {
        Text("Waiting for a connected guest agent and reported capabilities before requests can be queued.")
          .font(.caption)
          .foregroundStyle(.secondary)
      }
    }
    .alert("Close guest window?", isPresented: $isCloseWindowConfirmationPresented) {
      Button("Cancel", role: .cancel) {}
      Button("Close", role: .destructive) {
        let targetWindowID = pendingCloseWindowID
        pendingCloseWindowID = ""
        Task {
          if await onCloseWindow(targetWindowID) {
            windowID = ""
          }
        }
      }
    } message: {
      Text(
        "Queue a capability-gated close request for window \(pendingCloseWindowID); the effect depends on guest-agent acknowledgement."
      )
    }
  }

  @ViewBuilder
  private func commandLabel(_ title: String, systemImage: String) -> some View {
    if isSending {
      ProgressView()
        .controlSize(.small)
    } else {
      Label(title, systemImage: systemImage)
    }
  }

  private func canSend(capability: String) -> Bool {
    status.connected && status.runtime?.capabilities.contains(capability) == true
  }

  private var canSubmitDisplayResize: Bool {
    guard canSend(capability: "display-resize") else {
      return false
    }

    guard let width = UInt32(displayWidth.trimmingCharacters(in: .whitespacesAndNewlines)),
      let height = UInt32(displayHeight.trimmingCharacters(in: .whitespacesAndNewlines)),
      let scale = UInt16(displayScale.trimmingCharacters(in: .whitespacesAndNewlines))
    else {
      return false
    }

    return width > 0 && height > 0 && scale > 0
  }

  private var canSubmitApplicationID: Bool {
    canSend(capability: "applications")
      && !applicationID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
  }

  private var canSubmitWindowID: Bool {
    canSend(capability: "windows")
      && !windowID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
  }

  private var canSubmitInlineFileDrop: Bool {
    canSend(capability: "drag-drop") && inlineFileDropValidationMessage == nil
  }

  private var inlineFileDropValidationMessage: String? {
    guard isSafeInlineDropFileName else {
      return "Enter a file name without path separators."
    }

    guard !trimmedDropContents.isEmpty else {
      return "Enter file contents to drop."
    }

    guard trimmedDropContents.utf8.count <= Self.maxInlineFileDropBytes else {
      return "Inline file contents must be 64 KB or less."
    }

    return nil
  }

  private var isSafeInlineDropFileName: Bool {
    let fileName = trimmedDropFileName
    return !fileName.isEmpty
      && fileName != "."
      && fileName != ".."
      && fileName.rangeOfCharacter(from: CharacterSet(charactersIn: "/\\")) == nil
  }

  private var trimmedDropFileName: String {
    dropFileName.trimmingCharacters(in: .whitespacesAndNewlines)
  }

  private var trimmedDropContents: String {
    dropContents.trimmingCharacters(in: .whitespacesAndNewlines)
  }
}

private struct GuestClipboardTelemetryView: View {
  var clipboard: GuestClipboardSnapshot?
  var pasteboard: HostPasteboardWriting = SystemHostPasteboard()

  private var canCopyToHost: Bool {
    !(clipboard?.text.isEmpty ?? true)
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      Text("Guest Clipboard")
        .font(.callout)
        .foregroundStyle(.secondary)

      if let clipboard {
        VStack(alignment: .leading, spacing: 6) {
          GuestToolsFactRow(title: "Updated", value: unixTimeText(clipboard.updatedAtUnix))
          Text(clipboard.text.isEmpty ? "Empty text" : clipboard.text)
            .font(.callout)
            .lineLimit(3)
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(8)
            .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 6))

          HStack {
            Spacer()
            Button {
              copyGuestClipboardToHost(clipboard, into: pasteboard)
            } label: {
              Label("Copy to Mac clipboard", systemImage: "doc.on.clipboard")
            }
            .disabled(!canCopyToHost)
          }
          .controlSize(.small)
        }
      } else {
        Text("No guest-origin clipboard text has been reported.")
          .font(.callout)
          .foregroundStyle(.secondary)
      }
    }
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct GuestToolsAgentUpdateView: View {
  var update: GuestToolsAgentUpdate?

  var body: some View {
    if let update {
      VStack(alignment: .leading, spacing: 6) {
        Text("Agent Update")
          .font(.subheadline)
          .fontWeight(.semibold)
        GuestToolsFactRow(title: "Current", value: update.currentVersion)
        GuestToolsFactRow(title: "Available", value: update.availableVersion)
        if let downloadURL = update.downloadURL {
          GuestToolsFactRow(title: "URL", value: downloadURL)
        }
        GuestToolsFactRow(title: "Signature", value: update.signature == nil ? "None" : "Present")
        GuestToolsFactRow(title: "Observed", value: unixTimeText(update.observedAtUnix))
      }
    }
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct LastGuestToolsCommandResultView: View {
  var result: GuestToolsCommandResult?
  var isSending: Bool
  var onLaunchApplication: (String) async -> Bool
  var onFocusWindow: (String) async -> Bool
  var onCloseWindow: (String) async -> Bool
  var onOpenWindowProxy: (GuestToolsWindowAction) async -> Bool

  @State private var pendingCloseWindow: GuestToolsWindowAction?

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      Text("Last Command Result")
        .font(.callout)
        .foregroundStyle(.secondary)

      if let result {
        VStack(alignment: .leading, spacing: 6) {
          HStack(spacing: 8) {
            Label(
              result.ok ? "OK" : "Failed",
              systemImage: result.ok ? "checkmark.circle" : "xmark.octagon"
            )
            .foregroundStyle(result.ok ? .green : .red)
            Text(result.capability ?? "Unknown capability")
              .foregroundStyle(.secondary)
          }
          .font(.callout.weight(.medium))

          GuestToolsFactRow(title: "Request", value: result.requestID)
          GuestToolsFactRow(title: "Message", value: result.message ?? "No message")
          GuestToolsFactRow(title: "Error", value: result.errorCode ?? "None")
          if let backend = result.backendSourceSummary {
            GuestToolsFactRow(title: "Backend source", value: backend)
          }
          GuestToolsFactRow(title: "Result", value: result.result?.displayText ?? "None")
          GuestToolsFactRow(title: "Metadata", value: result.metadata?.displayText ?? "None")
          GuestToolsFactRow(title: "Completed", value: unixTimeText(result.completedAtUnix))

          GuestToolsApplicationActionList(
            applications: result.applicationActions,
            isSending: isSending,
            onLaunch: onLaunchApplication
          )
          GuestToolsWindowActionList(
            windows: result.windowActions,
            isSending: isSending,
            onFocus: onFocusWindow,
            onOpenProxy: onOpenWindowProxy,
            onRequestClose: { pendingCloseWindow = $0 }
          )
        }
        .padding(10)
        .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
      } else {
        Text("No command result has been reported by the guest agent.")
          .font(.callout)
          .foregroundStyle(.secondary)
      }
    }
    .alert(
      "Close guest window?",
      isPresented: Binding(
        get: { pendingCloseWindow != nil },
        set: { if !$0 { pendingCloseWindow = nil } }
      )
    ) {
      Button("Cancel", role: .cancel) {
        pendingCloseWindow = nil
      }
      Button("Close", role: .destructive) {
        let windowID = pendingCloseWindow?.id ?? ""
        pendingCloseWindow = nil
        Task { _ = await onCloseWindow(windowID) }
      }
    } message: {
      Text(
        "Queue a capability-gated close request for \(pendingCloseWindow?.title ?? "this guest window")."
      )
    }
  }

  private func unixTimeText(_ value: UInt64) -> String {
    Date(timeIntervalSince1970: TimeInterval(value))
      .formatted(date: .abbreviated, time: .shortened)
  }
}

private struct GuestToolsApplicationActionList: View {
  var applications: [GuestToolsApplicationAction]
  var isSending: Bool
  var onLaunch: (String) async -> Bool

  var body: some View {
    if !applications.isEmpty {
      VStack(alignment: .leading, spacing: 6) {
        Text("Applications")
          .font(.caption.weight(.medium))
          .foregroundStyle(.secondary)

        ForEach(applications) { application in
          HStack(alignment: .center, spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
              Text(application.name)
                .font(.callout.weight(.medium))
                .lineLimit(1)
              Text(applicationDetail(application))
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
            }

            Spacer()

            Button {
              Task { _ = await onLaunch(application.id) }
            } label: {
              Label("Launch", systemImage: "play.circle")
            }
            .controlSize(.small)
            .disabled(isSending)
          }
        }
      }
      .padding(.top, 4)
    }
  }

  private func applicationDetail(_ application: GuestToolsApplicationAction) -> String {
    var parts = [application.id]
    if let source = application.source {
      parts.append(source)
    }
    if application.launched == true {
      parts.append("launched")
    }
    return parts.joined(separator: " - ")
  }
}

private struct GuestToolsWindowActionList: View {
  var windows: [GuestToolsWindowAction]
  var isSending: Bool
  var onFocus: (String) async -> Bool
  var onOpenProxy: (GuestToolsWindowAction) async -> Bool
  var onRequestClose: (GuestToolsWindowAction) -> Void

  var body: some View {
    if !windows.isEmpty {
      VStack(alignment: .leading, spacing: 6) {
        Text("Windows")
          .font(.caption.weight(.medium))
          .foregroundStyle(.secondary)

        ForEach(windows) { window in
          HStack(alignment: .center, spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
              Text(window.title)
                .font(.callout.weight(.medium))
                .lineLimit(1)
              Text(windowDetail(window))
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
            }

            Spacer()

            Button {
              Task { _ = await onFocus(window.id) }
            } label: {
              Label("Focus", systemImage: "scope")
            }
            .controlSize(.small)
            .disabled(isSending)

            Button {
              Task { _ = await onOpenProxy(window) }
            } label: {
              Label("Proxy", systemImage: "macwindow.on.rectangle")
            }
            .controlSize(.small)
            .disabled(window.bounds == nil)
            .help(
              window.bounds == nil
                ? "Guest window bounds are required before opening a proxy shell."
                : "Open a macOS proxy shell sized from the guest window bounds."
            )

            Button(role: .destructive) {
              onRequestClose(window)
            } label: {
              Label("Close", systemImage: "xmark.circle")
            }
            .controlSize(.small)
            .disabled(isSending)
          }
        }
      }
      .padding(.top, 4)
    }
  }

  private func windowDetail(_ window: GuestToolsWindowAction) -> String {
    var parts = [window.id]
    if let source = window.source {
      parts.append(source)
    }
    if let pid = window.pid {
      parts.append("pid \(pid)")
    }
    if let bounds = window.bounds {
      parts.append(bounds.displayText)
    }
    if window.cropFrameSummaryPath != nil {
      parts.append("crop artifact")
    }
    if window.focused == true {
      parts.append("focused")
    }
    if window.closed == true {
      parts.append("closed")
    }
    return parts.joined(separator: " - ")
  }
}

private struct GuestToolsStatusBadge: View {
  var title: String
  var value: String
  var systemImage: String

  var body: some View {
    Label {
      VStack(alignment: .leading, spacing: 2) {
        Text(title)
          .font(.caption)
          .foregroundStyle(.secondary)
        Text(value)
          .font(.callout.weight(.medium))
      }
    } icon: {
      Image(systemName: systemImage)
        .foregroundStyle(.secondary)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
  }
}

private struct GuestToolsFactRow: View {
  var title: String
  var value: String

  var body: some View {
    HStack(alignment: .firstTextBaseline) {
      Text(title)
        .foregroundStyle(.secondary)
        .frame(width: 90, alignment: .leading)
      Text(value)
        .lineLimit(2)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
    .font(.callout)
  }
}

private struct CapabilityList: View {
  var capabilities: [GuestToolsCapability]

  var body: some View {
    if capabilities.isEmpty {
      Text("No allowed capabilities reported.")
        .font(.callout)
        .foregroundStyle(.secondary)
    } else {
      LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 8)], spacing: 8) {
        ForEach(capabilities) { capability in
          VStack(alignment: .leading, spacing: 2) {
            Text(capability.name)
              .font(.caption.weight(.medium))
              .lineLimit(1)
            Text("\(capability.enabledBy), v\(capability.maxVersion)")
              .font(.caption2)
              .foregroundStyle(.secondary)
              .lineLimit(1)
          }
          .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
          .padding(8)
          .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
        }
      }
    }
  }
}

private struct PathPickerButton: View {
  enum Mode {
    case file
    case directory
    case fileOrDirectory
    case saveFile(defaultName: String)
  }

  var title: String
  var mode: Mode
  @Binding var path: String

  var body: some View {
    Button {
      choosePath()
    } label: {
      Label("Choose", systemImage: systemImage)
    }
    .help(title)
  }

  private var systemImage: String {
    switch mode {
    case .file, .fileOrDirectory, .saveFile:
      return "doc.badge.ellipsis"
    case .directory:
      return "folder"
    }
  }

  private func choosePath() {
    switch mode {
    case .saveFile(let defaultName):
      let panel = NSSavePanel()
      panel.title = title
      panel.nameFieldStringValue = suggestedName(defaultName)
      if panel.runModal() == .OK, let url = panel.url {
        path = url.path
      }
    case .file, .directory, .fileOrDirectory:
      let panel = NSOpenPanel()
      panel.title = title
      panel.allowsMultipleSelection = false
      panel.canChooseFiles = mode.canChooseFiles
      panel.canChooseDirectories = mode.canChooseDirectories
      panel.canCreateDirectories = mode.canChooseDirectories
      if panel.runModal() == .OK, let url = panel.url {
        path = url.path
      }
    }
  }

  private func suggestedName(_ fallback: String) -> String {
    let currentPath = path.trimmingCharacters(in: .whitespacesAndNewlines)
    return currentPath.isEmpty ? fallback : URL(fileURLWithPath: currentPath).lastPathComponent
  }
}

extension PathPickerButton.Mode {
  fileprivate var canChooseFiles: Bool {
    switch self {
    case .file, .fileOrDirectory:
      return true
    case .directory, .saveFile:
      return false
    }
  }

  fileprivate var canChooseDirectories: Bool {
    switch self {
    case .directory, .fileOrDirectory:
      return true
    case .file, .saveFile:
      return false
    }
  }
}
