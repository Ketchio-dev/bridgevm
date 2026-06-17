import SwiftUI

struct DashboardView: View {
  @ObservedObject var model: DashboardViewModel
  @State private var isCreateSheetPresented = false
  @State private var cloneSheetVirtualMachine: VirtualMachine?
  @State private var deleteConfirmationVirtualMachine: VirtualMachine?

  var body: some View {
    NavigationSplitView {
      SidebarView(
        model: model,
        onCreate: {
          isCreateSheetPresented = true
        }
      )
      .navigationSplitViewColumnWidth(min: 300, ideal: 340, max: 420)
    } detail: {
      if let virtualMachine = model.selectedVirtualMachine {
        VMDetailView(
          virtualMachine: virtualMachine,
          isWorking: model.activeActionID == virtualMachine.id,
          isCloning: model.cloningVirtualMachineID == virtualMachine.id,
          isOpeningConsole: model.openingConsoleID == virtualMachine.id,
          readinessReport: model.readinessReport(for: virtualMachine),
          isLoadingReadinessReport: model.loadingReadinessReportID == virtualMachine.id,
          readinessReportError: model.readinessReportError(for: virtualMachine),
          bootMediaStatus: model.bootMediaStatus(for: virtualMachine),
          isLoadingBootMediaStatus: model.loadingBootMediaStatusID == virtualMachine.id,
          isImportingBootMedia: model.importingBootMediaID == virtualMachine.id,
          isVerifyingBootMedia: model.verifyingBootMediaID == virtualMachine.id,
          isPlanningBootMediaDownload: model.planningBootMediaDownloadID == virtualMachine.id,
          isDownloadingBootMedia: model.downloadingBootMediaID == virtualMachine.id,
          bootMediaStatusError: model.bootMediaStatusError(for: virtualMachine),
          guestToolsStatus: model.guestToolsStatus(for: virtualMachine),
          guestToolsProvisioning: model.guestToolsProvisioning(for: virtualMachine),
          isLoadingGuestToolsStatus: model.loadingGuestToolsStatusID == virtualMachine.id,
          isSendingGuestToolsCommand: model.sendingGuestToolsCommandID == virtualMachine.id,
          guestToolsStatusError: model.guestToolsStatusError(for: virtualMachine),
          guestToolsProvisioningError: model.guestToolsProvisioningError(for: virtualMachine),
          sharedFolderList: model.sharedFolderList(for: virtualMachine),
          isLoadingSharedFolders: model.loadingSharedFoldersID == virtualMachine.id,
          isAddingSharedFolder: model.addingSharedFolderID == virtualMachine.id,
          isRemovingSharedFolder: model.removingSharedFolderID == virtualMachine.id,
          sharedFolderError: model.sharedFolderError(for: virtualMachine),
          runnerStatus: model.runnerStatus(for: virtualMachine),
          isLoadingRunnerStatus: model.loadingRunnerStatusID == virtualMachine.id,
          runnerStatusError: model.runnerStatusError(for: virtualMachine),
          snapshotPreflightStatus: model.snapshotPreflightStatus(for: virtualMachine),
          isLoadingSnapshotPreflightStatus: model.loadingSnapshotPreflightStatusID
            == virtualMachine.id,
          snapshotPreflightStatusError: model.snapshotPreflightStatusError(for: virtualMachine),
          snapshots: model.snapshots(for: virtualMachine),
          isLoadingSnapshots: model.loadingSnapshotsID == virtualMachine.id,
          snapshotError: model.snapshotError(for: virtualMachine),
          snapshotChain: model.snapshotChain(for: virtualMachine),
          isLoadingSnapshotChain: model.loadingSnapshotChainID == virtualMachine.id,
          snapshotChainError: model.snapshotChainError(for: virtualMachine),
          diskPreparation: model.diskPreparation(for: virtualMachine),
          isPreparingDisk: model.preparingDiskID == virtualMachine.id,
          diskPreparationError: model.diskPreparationError(for: virtualMachine),
          diskCreation: model.diskCreation(for: virtualMachine),
          isCreatingDisk: model.creatingDiskID == virtualMachine.id,
          diskCreationError: model.diskCreationError(for: virtualMachine),
          diskInspection: model.diskInspection(for: virtualMachine),
          isInspectingDisk: model.inspectingDiskID == virtualMachine.id,
          diskInspectionError: model.diskInspectionError(for: virtualMachine),
          diskVerification: model.diskVerification(for: virtualMachine),
          isVerifyingDisk: model.verifyingDiskID == virtualMachine.id,
          diskVerificationError: model.diskVerificationError(for: virtualMachine),
          diskCompaction: model.diskCompaction(for: virtualMachine),
          isCompactingDisk: model.compactingDiskID == virtualMachine.id,
          diskCompactionError: model.diskCompactionError(for: virtualMachine),
          metadataRepair: model.metadataRepair(for: virtualMachine),
          isRepairingMetadata: model.repairingMetadataID == virtualMachine.id,
          metadataRepairError: model.metadataRepairError(for: virtualMachine),
          manifestMigration: model.manifestMigration(for: virtualMachine),
          isCheckingManifestMigration: model.checkingManifestMigrationID == virtualMachine.id,
          manifestMigrationError: model.manifestMigrationError(for: virtualMachine),
          snapshotRestoreResult: model.snapshotRestoreResult(for: virtualMachine),
          isRestoringSnapshot: model.restoringSnapshotID == virtualMachine.id,
          snapshotRestoreError: model.snapshotRestoreError(for: virtualMachine),
          snapshotCreation: model.snapshotCreation(for: virtualMachine),
          isCreatingSnapshot: model.creatingSnapshotID == virtualMachine.id,
          snapshotCreationError: model.snapshotCreationError(for: virtualMachine),
          snapshotDiskCreation: model.snapshotDiskCreation(for: virtualMachine),
          isCreatingSnapshotDisk: model.creatingSnapshotDiskID == virtualMachine.id,
          snapshotDiskCreationError: model.snapshotDiskCreationError(for: virtualMachine),
          applicationConsistentSnapshotExecution:
            model
            .applicationConsistentSnapshotExecution(for: virtualMachine),
          isExecutingApplicationConsistentSnapshot: model
            .executingApplicationConsistentSnapshotID == virtualMachine.id,
          applicationConsistentSnapshotExecutionError:
            model
            .applicationConsistentSnapshotExecutionError(for: virtualMachine),
          vmExport: model.vmExport(for: virtualMachine),
          isExportingVirtualMachine: model.exportingVirtualMachineID == virtualMachine.id,
          vmExportError: model.vmExportError(for: virtualMachine),
          lastVMImport: model.lastVMImport,
          isImportingVirtualMachine: model.isImportingVirtualMachine,
          vmImportError: model.vmImportError,
          diagnosticBundle: model.diagnosticBundle(for: virtualMachine),
          isCreatingDiagnosticBundle: model.creatingDiagnosticBundleID == virtualMachine.id,
          diagnosticBundleError: model.diagnosticBundleError(for: virtualMachine),
          performanceBaseline: model.performanceBaseline(for: virtualMachine),
          isCreatingPerformanceBaseline: model.creatingPerformanceBaselineID == virtualMachine.id,
          performanceBaselineError: model.performanceBaselineError(for: virtualMachine),
          performanceSample: model.performanceSample(for: virtualMachine),
          isCreatingPerformanceSample: model.creatingPerformanceSampleID == virtualMachine.id,
          performanceSampleError: model.performanceSampleError(for: virtualMachine),
          qmpStatus: model.qmpStatus(for: virtualMachine),
          qmpStatusError: model.qmpStatusError(for: virtualMachine),
          qemuLaunchPlan: model.qemuLaunchPlan(for: virtualMachine),
          isLoadingQemuLaunchPlan: model.loadingQemuLaunchPlanID == virtualMachine.id,
          qemuLaunchPlanError: model.qemuLaunchPlanError(for: virtualMachine),
          qemuLog: model.logView(kind: .qemu, for: virtualMachine),
          serialLog: model.logView(kind: .serial, for: virtualMachine),
          isLoadingLog: model.loadingLogViewID == virtualMachine.id,
          logViewError: model.logViewError(for: virtualMachine),
          lifecycleActions: model.lifecycleActions(for: virtualMachine),
          lifecyclePlan: model.lifecyclePlan(for: virtualMachine),
          isLoadingLifecyclePlan: model.loadingLifecyclePlanID == virtualMachine.id,
          lifecyclePlanError: model.lifecyclePlanError(for: virtualMachine),
          portForwardList: model.portForwardList(for: virtualMachine),
          isLoadingPortForwards: model.loadingPortForwardsID == virtualMachine.id,
          isAddingPortForward: model.addingPortForwardID == virtualMachine.id,
          isRemovingPortForward: model.removingPortForwardID == virtualMachine.id,
          portForwardError: model.portForwardError(for: virtualMachine),
          openPortPlan: model.openPortPlan(for: virtualMachine),
          isLoadingOpenPortPlan: model.loadingOpenPortPlanID == virtualMachine.id,
          openPortPlanError: model.openPortPlanError(for: virtualMachine),
          sshPlan: model.sshPlan(for: virtualMachine),
          isLoadingSSHPlan: model.loadingSSHPlanID == virtualMachine.id,
          sshPlanError: model.sshPlanError(for: virtualMachine),
          networkPlan: model.networkPlan(for: virtualMachine),
          isLoadingNetworkPlan: model.loadingNetworkPlanID == virtualMachine.id,
          networkPlanError: model.networkPlanError(for: virtualMachine),
          onPrimaryAction: {
            await model.performPrimaryAction(on: virtualMachine)
          },
          onClone: {
            cloneSheetVirtualMachine = virtualMachine
          },
          onOpenConsole: {
            await model.openConsole(for: virtualMachine)
          },
          onShowDisplay: {
            await model.showDisplay(for: virtualMachine)
          },
          onStop: {
            await model.perform(.stop, on: virtualMachine)
          },
          onRestart: {
            await model.perform(.restart, on: virtualMachine)
          },
          onPerformLifecycleAction: { action in
            await model.perform(action, on: virtualMachine)
          },
          onInspectLifecyclePlan: { action in
            await model.loadLifecyclePlan(action: action, for: virtualMachine)
          },
          onRefreshPortForwards: {
            await model.loadPortForwards(for: virtualMachine)
          },
          onAddPortForward: { host, guest in
            await model.addPortForward(host: host, guest: guest, for: virtualMachine)
          },
          onRemovePortForward: { host, guest in
            await model.removePortForward(host: host, guest: guest, for: virtualMachine)
          },
          onInspectOpenPortPlan: { guestPort, scheme in
            await model.loadOpenPortPlan(
              guestPort: guestPort,
              scheme: scheme,
              for: virtualMachine
            )
          },
          onInspectSSHPlan: { user in
            await model.loadSSHPlan(user: user, for: virtualMachine)
          },
          onRefreshNetworkPlan: {
            await model.loadNetworkPlan(for: virtualMachine)
          },
          onRefreshBootMediaStatus: {
            await model.loadBootMediaStatus(for: virtualMachine)
          },
          onImportBootMedia: { sourcePath, kind in
            await model.importBootMedia(
              sourcePath: sourcePath,
              kind: kind,
              for: virtualMachine
            )
          },
          onVerifyBootMedia: { expectedSHA256, kind in
            await model.verifyBootMedia(
              expectedSHA256: expectedSHA256,
              kind: kind,
              for: virtualMachine
            )
          },
          onPlanBootMediaDownload: { url, expectedSHA256, kind in
            await model.planBootMediaDownload(
              url: url,
              expectedSHA256: expectedSHA256,
              kind: kind,
              for: virtualMachine
            )
          },
          onDownloadBootMedia: { kind in
            await model.downloadBootMedia(
              kind: kind,
              for: virtualMachine
            )
          },
          onRefreshGuestToolsStatus: {
            await model.loadGuestToolsStatus(for: virtualMachine)
          },
          onRefreshSharedFolders: {
            await model.loadSharedFolders(for: virtualMachine)
          },
          onAddSharedFolder: { name, hostPath, readOnly, hostPathToken in
            await model.addSharedFolder(
              name: name,
              hostPath: hostPath,
              readOnly: readOnly,
              hostPathToken: hostPathToken,
              for: virtualMachine
            )
          },
          onRemoveSharedFolder: { shareName in
            await model.removeSharedFolder(named: shareName, for: virtualMachine)
          },
          onMountApprovedSharedFolder: { shareName in
            await model.mountApprovedSharedFolder(named: shareName, for: virtualMachine)
          },
          onUnmountApprovedSharedFolder: { shareName in
            await model.unmountApprovedSharedFolder(named: shareName, for: virtualMachine)
          },
          onSendGuestToolsCommand: { command in
            await model.sendGuestToolsCommand(command, for: virtualMachine)
          },
          onSyncGuestTime: {
            await model.syncGuestTime(for: virtualMachine)
          },
          onSetClipboardText: { text in
            await model.sendClipboardText(text, for: virtualMachine)
          },
          onResizeDisplay: { width, height, scale in
            await model.resizeDisplay(
              width: width,
              height: height,
              scale: scale,
              for: virtualMachine
            )
          },
          onLaunchApplication: { applicationID in
            await model.launchApplication(id: applicationID, for: virtualMachine)
          },
          onFocusWindow: { windowID in
            await model.focusWindow(id: windowID, for: virtualMachine)
          },
          onCloseWindow: { windowID in
            await model.closeWindow(id: windowID, for: virtualMachine)
          },
          onSendInlineFileDrop: { fileName, contents in
            await model.sendInlineFileDrop(
              fileName: fileName,
              contents: contents,
              for: virtualMachine
            )
          },
          onPrepareRun: {
            await model.prepareRun(for: virtualMachine)
          },
          onRefreshRunnerStatus: {
            await model.loadRunnerStatus(for: virtualMachine)
          },
          onRefreshSnapshotPreflightStatus: {
            await model.loadSnapshotPreflightStatus(for: virtualMachine)
          },
          onRefreshSnapshots: {
            await model.loadSnapshots(for: virtualMachine)
          },
          onRefreshSnapshotChain: {
            await model.loadSnapshotChain(for: virtualMachine)
          },
          onPreparePrimaryDisk: {
            await model.preparePrimaryDisk(for: virtualMachine)
          },
          onCreatePrimaryDisk: {
            await model.createPrimaryDisk(for: virtualMachine)
          },
          onInspectPrimaryDisk: {
            await model.inspectPrimaryDisk(for: virtualMachine)
          },
          onVerifyActiveDisk: {
            await model.verifyActiveDisk(for: virtualMachine)
          },
          onCompactActiveDisk: {
            await model.compactActiveDisk(for: virtualMachine)
          },
          onRepairMetadata: {
            await model.repairMetadata(for: virtualMachine)
          },
          onCheckManifestMigration: {
            await model.checkManifestMigration(for: virtualMachine)
          },
          onRestoreSnapshot: { snapshotName in
            await model.restoreSnapshot(named: snapshotName, for: virtualMachine)
          },
          onCreateSnapshot: { snapshotName, kind in
            await model.createSnapshot(named: snapshotName, kind: kind, for: virtualMachine)
          },
          onCreateSnapshotDisk: { snapshotName in
            await model.createSnapshotDisk(named: snapshotName, for: virtualMachine)
          },
          onExecuteApplicationConsistentSnapshot: { snapshotName, freezeTimeoutMillis in
            await model.executeApplicationConsistentSnapshot(
              named: snapshotName,
              freezeTimeoutMillis: freezeTimeoutMillis,
              for: virtualMachine
            )
          },
          onExportVirtualMachine: { output in
            await model.exportVirtualMachine(output: output, for: virtualMachine)
          },
          onImportVirtualMachine: { input, name in
            await model.importVirtualMachine(input: input, name: name)
          },
          onCreateDiagnosticBundle: { output in
            await model.createDiagnosticBundle(output: output, for: virtualMachine)
          },
          onCreatePerformanceBaseline: { output in
            await model.createPerformanceBaseline(output: output, for: virtualMachine)
          },
          onCreatePerformanceSample: { output, artifactBytes, iterations, sync in
            await model.createPerformanceSample(
              output: output,
              artifactBytes: artifactBytes,
              iterations: iterations,
              sync: sync,
              for: virtualMachine
            )
          },
          onRefreshQemuLaunchPlan: {
            await model.loadQemuLaunchPlan(for: virtualMachine)
          },
          onLoadLog: { kind in
            await model.loadLogView(kind: kind, for: virtualMachine)
          }
        )
        .toolbar {
          Button(role: .destructive) {
            deleteConfirmationVirtualMachine = virtualMachine
          } label: {
            if model.deletingVirtualMachineID == virtualMachine.id {
              ProgressView()
                .controlSize(.small)
            } else {
              Label("Delete", systemImage: "trash")
            }
          }
          .disabled(
            model.deletingVirtualMachineID == virtualMachine.id
              || virtualMachine.status != .stopped
          )
          .help(
            virtualMachine.status == .stopped
              ? "Delete VM metadata"
              : "Stop the VM before deleting it"
          )
        }
      } else {
        ContentUnavailableView(
          "No Virtual Machines",
          systemImage: "desktopcomputer",
          description: Text("Create or connect bridgevmd inventory to populate this dashboard.")
        )
      }
    }
    .task {
      await model.load()
    }
    .task(id: model.selection) {
      guard let virtualMachine = model.selectedVirtualMachine else {
        return
      }
      await model.loadReadinessReport(for: virtualMachine)
    }
    .sheet(isPresented: $isCreateSheetPresented) {
      CreateVirtualMachineSheet(model: model) {
        isCreateSheetPresented = false
      }
    }
    .sheet(item: $cloneSheetVirtualMachine) { virtualMachine in
      CloneVirtualMachineSheet(model: model, virtualMachine: virtualMachine) {
        cloneSheetVirtualMachine = nil
      }
    }
    .alert(
      "BridgeVM",
      isPresented: Binding(
        get: { model.alertMessage != nil },
        set: { if !$0 { model.alertMessage = nil } }
      ),
      actions: {
        Button("OK", role: .cancel) {
          model.alertMessage = nil
        }
      },
      message: {
        Text(model.alertMessage ?? "")
      }
    )
    .alert(
      "Delete VM metadata?",
      isPresented: Binding(
        get: { deleteConfirmationVirtualMachine != nil },
        set: { if !$0 { deleteConfirmationVirtualMachine = nil } }
      ),
      presenting: deleteConfirmationVirtualMachine,
      actions: { virtualMachine in
        Button("Cancel", role: .cancel) {
          deleteConfirmationVirtualMachine = nil
        }
        Button("Delete", role: .destructive) {
          deleteConfirmationVirtualMachine = nil
          Task { await model.deleteVirtualMachine(virtualMachine) }
        }
      },
      message: { virtualMachine in
        Text("Remove metadata for \(virtualMachine.name). This does not delete disk data.")
      }
    )
  }
}

private struct CloneVirtualMachineSheet: View {
  @ObservedObject var model: DashboardViewModel
  var virtualMachine: VirtualMachine
  var onClose: () -> Void

  @State private var name: String
  @State private var linked = false

  init(
    model: DashboardViewModel,
    virtualMachine: VirtualMachine,
    onClose: @escaping () -> Void
  ) {
    self.model = model
    self.virtualMachine = virtualMachine
    self.onClose = onClose
    _name = State(initialValue: "\(virtualMachine.name) Copy")
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 18) {
      VStack(alignment: .leading, spacing: 4) {
        Text("Clone Virtual Machine")
          .font(.title3.weight(.semibold))
        Text("Create a stopped copy of \(virtualMachine.name).")
          .font(.callout)
          .foregroundStyle(.secondary)
      }

      VStack(alignment: .leading, spacing: 8) {
        TextField("Clone name", text: $name)
          .textFieldStyle(.roundedBorder)

        Toggle(isOn: $linked) {
          VStack(alignment: .leading, spacing: 2) {
            Text("Linked clone")
            Text("Create an overlay backed by the source disk instead of copying disk data.")
              .font(.caption)
              .foregroundStyle(.secondary)
          }
        }

        HStack(spacing: 12) {
          Label(virtualMachine.guest, systemImage: "cpu")
          Label(virtualMachine.mode.title, systemImage: "bolt.horizontal")
          Label(
            "\(virtualMachine.resources.cpuCount) CPU / \(virtualMachine.resources.memoryGB) GB",
            systemImage: "memorychip"
          )
        }
        .font(.caption)
        .foregroundStyle(.secondary)
      }

      HStack {
        Spacer()

        Button("Cancel", role: .cancel) {
          onClose()
        }
        .disabled(isCloning)

        Button {
          Task {
            if await model.cloneVirtualMachine(
              name: name,
              linked: linked,
              for: virtualMachine
            ) {
              onClose()
            }
          }
        } label: {
          if isCloning {
            ProgressView()
              .controlSize(.small)
          } else {
            Text("Clone")
          }
        }
        .keyboardShortcut(.defaultAction)
        .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || isCloning)
      }
    }
    .padding(22)
    .frame(width: 430)
  }

  private var isCloning: Bool {
    model.cloningVirtualMachineID == virtualMachine.id
  }
}

private struct SidebarView: View {
  @ObservedObject var model: DashboardViewModel
  var onCreate: () -> Void

  var body: some View {
    VStack(spacing: 0) {
      DashboardHeader(
        totalCount: model.virtualMachines.count,
        runningCount: model.runningCount,
        fastModeCount: model.fastModeCount,
        sourceTitle: model.inventorySourceTitle,
        refreshStatusText: model.refreshStatusText,
        isRefreshing: model.isLoading,
        onCreate: onCreate,
        onRefresh: {
          await model.load()
        }
      )
      .padding(16)

      SearchField(text: $model.searchText)
        .padding(.horizontal, 16)
        .padding(.bottom, 12)

      if model.isLoading && model.virtualMachines.isEmpty {
        ProgressView()
          .frame(maxWidth: .infinity, maxHeight: .infinity)
      } else {
        List(model.filteredVirtualMachines, selection: $model.selection) { virtualMachine in
          VMRowView(
            virtualMachine: virtualMachine,
            summary: model.cardSummary(for: virtualMachine)
          )
          .tag(virtualMachine.id)
        }
        .listStyle(.sidebar)
      }
    }
    .background(Color(nsColor: .windowBackgroundColor))
  }
}

private struct DashboardHeader: View {
  var totalCount: Int
  var runningCount: Int
  var fastModeCount: Int
  var sourceTitle: String
  var refreshStatusText: String
  var isRefreshing: Bool
  var onCreate: () -> Void
  var onRefresh: () async -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Label("BridgeVM", systemImage: "square.stack.3d.up")
          .font(.title2.weight(.semibold))

        Spacer()

        Button {
          onCreate()
        } label: {
          Image(systemName: "plus")
        }
        .buttonStyle(.bordered)
        .help("Create VM")
      }

      HStack(spacing: 8) {
        Label(sourceTitle, systemImage: sourceIcon)
          .font(.caption)
          .foregroundStyle(.secondary)
          .lineLimit(1)

        Text(refreshStatusText)
          .font(.caption)
          .foregroundStyle(refreshStatusText.hasPrefix("Refresh failed") ? .red : .secondary)
          .lineLimit(1)
          .truncationMode(.middle)

        Spacer(minLength: 8)

        Button {
          Task { await onRefresh() }
        } label: {
          if isRefreshing {
            ProgressView()
              .controlSize(.small)
          } else {
            Image(systemName: "arrow.clockwise")
          }
        }
        .buttonStyle(.borderless)
        .disabled(isRefreshing)
        .help("Refresh inventory")
      }
      .frame(minHeight: 22)

      HStack(spacing: 10) {
        StatPill(title: "Total", value: totalCount)
        StatPill(title: "Running", value: runningCount)
        StatPill(title: "Fast", value: fastModeCount)
      }
    }
  }

  private var sourceIcon: String {
    sourceTitle.localizedCaseInsensitiveContains("mock") ? "shippingbox" : "server.rack"
  }
}

private struct CreateVirtualMachineSheet: View {
  @ObservedObject var model: DashboardViewModel
  var onClose: () -> Void

  @State private var name = ""
  @State private var selectedTemplateID: BootTemplate.ID?
  @State private var recommendationChoiceID = "template"

  private var selectedTemplate: BootTemplate? {
    model.bootTemplates.first(where: { $0.id == selectedTemplateID }) ?? model.bootTemplates.first
  }

  private var recommendationChoice: GuestChoice? {
    switch recommendationChoiceID {
    case "windows-11-arm":
      return GuestChoice(os: "windows", version: "11", arch: "arm64")
    case "ubuntu-x86_64":
      return GuestChoice(os: "ubuntu", version: nil, arch: "x86_64")
    default:
      return selectedTemplate.map(GuestChoice.init)
    }
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 18) {
      HStack {
        VStack(alignment: .leading, spacing: 4) {
          Text("New Virtual Machine")
            .font(.title3.weight(.semibold))
          Text("Create a stopped VM bundle from a daemon boot template.")
            .font(.callout)
            .foregroundStyle(.secondary)
        }

        Spacer()
      }

      VStack(alignment: .leading, spacing: 12) {
        TextField("Name", text: $name)
          .textFieldStyle(.roundedBorder)

        if model.isLoadingBootTemplates && model.bootTemplates.isEmpty {
          HStack(spacing: 8) {
            ProgressView()
              .controlSize(.small)
            Text("Loading templates")
              .foregroundStyle(.secondary)
          }
          .frame(maxWidth: .infinity, alignment: .leading)
          .padding(.vertical, 8)
        } else {
          Picker("OS / Template", selection: selectedTemplateBinding) {
            ForEach(model.bootTemplates) { template in
              Text(template.guestTitle).tag(Optional(template.id))
            }
          }

          if let selectedTemplate {
            VStack(alignment: .leading, spacing: 8) {
              HStack {
                Label(selectedTemplate.engineMode.title, systemImage: "bolt.horizontal")
                Spacer()
                Text(selectedTemplate.mode.title)
                  .foregroundStyle(.secondary)
              }

              HStack {
                Label(selectedTemplate.mediaLabel, systemImage: "opticaldiscdrive")
                Spacer()
                Text(selectedTemplate.source.capitalized)
                  .foregroundStyle(.secondary)
              }

              Text(selectedTemplate.note)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

              Divider()

              Picker("Guidance for", selection: $recommendationChoiceID) {
                Text("Selected template").tag("template")
                Text("Windows 11 Arm").tag("windows-11-arm")
                Text("Ubuntu x86_64").tag("ubuntu-x86_64")
              }

              ModeRecommendationPanel(
                recommendation: model.modeRecommendation,
                isLoading: model.isLoadingModeRecommendation,
                errorMessage: model.modeRecommendationError
              )
            }
            .font(.callout)
            .padding(12)
            .background(
              Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
          }
        }
      }

      HStack {
        Spacer()

        Button("Cancel", role: .cancel) {
          onClose()
        }
        .disabled(model.isCreatingVirtualMachine)

        Button {
          Task {
            if await model.createVirtualMachine(name: name, templateID: selectedTemplateID) {
              onClose()
            }
          }
        } label: {
          if model.isCreatingVirtualMachine {
            ProgressView()
              .controlSize(.small)
          } else {
            Text("Create")
          }
        }
        .keyboardShortcut(.defaultAction)
        .disabled(
          name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            || selectedTemplate == nil
            || model.isCreatingVirtualMachine)
      }
    }
    .padding(22)
    .frame(width: 460)
    .task {
      await model.loadBootTemplates()
      selectedTemplateID = selectedTemplateID ?? model.bootTemplates.first?.id
      await model.loadModeRecommendation(for: recommendationChoice)
    }
    .onChange(of: model.bootTemplates) { _, templates in
      if selectedTemplateID == nil || !templates.contains(where: { $0.id == selectedTemplateID }) {
        selectedTemplateID = templates.first?.id
      }
      Task {
        await model.loadModeRecommendation(for: recommendationChoice)
      }
    }
    .onChange(of: selectedTemplateID) { _, _ in
      Task {
        await model.loadModeRecommendation(for: recommendationChoice)
      }
    }
    .onChange(of: recommendationChoiceID) { _, _ in
      Task {
        await model.loadModeRecommendation(for: recommendationChoice)
      }
    }
  }

  private var selectedTemplateBinding: Binding<BootTemplate.ID?> {
    Binding(
      get: { selectedTemplateID ?? model.bootTemplates.first?.id },
      set: { selectedTemplateID = $0 }
    )
  }
}

private struct ModeRecommendationPanel: View {
  var recommendation: ModeRecommendation?
  var isLoading: Bool
  var errorMessage: String?

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      HStack {
        Label("Recommendation", systemImage: "sparkles")
        Spacer()
        if isLoading {
          ProgressView()
            .controlSize(.small)
        } else if let recommendation {
          Text(recommendation.mode.title)
            .foregroundStyle(recommendation.fastModeAvailable ? .primary : .secondary)
        }
      }

      if let recommendation {
        Grid(alignment: .leading, horizontalSpacing: 14, verticalSpacing: 4) {
          GridRow {
            Text("Backend")
              .foregroundStyle(.secondary)
            Text(recommendation.backendGuidance)
          }
          GridRow {
            Text("Performance")
              .foregroundStyle(.secondary)
            Text(recommendation.performance)
          }
          GridRow {
            Text("Battery")
              .foregroundStyle(.secondary)
            Text(recommendation.batteryImpact)
          }
          GridRow {
            Text("Integration")
              .foregroundStyle(.secondary)
            Text(recommendation.integration)
          }
        }
        .font(.caption)

        Text(recommendation.message)
          .font(.caption)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)

        if let bootTemplate = recommendation.bootTemplate {
          Text("Recommended template: \(bootTemplate.id)")
            .font(.caption)
            .foregroundStyle(.secondary)
        }
      } else if let errorMessage {
        Text("Recommendation unavailable: \(errorMessage)")
          .font(.caption)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      } else if !isLoading {
        Text("Choose a template to request mode guidance.")
          .font(.caption)
          .foregroundStyle(.secondary)
      }
    }
  }
}

extension ModeRecommendation {
  fileprivate var backendGuidance: String {
    if message.localizedCaseInsensitiveContains("restricted backend") {
      return "Restricted backend"
    }

    return mode == .fast ? "Fast backend preferred" : "Compatibility backend"
  }
}

private struct StatPill: View {
  var title: String
  var value: Int

  var body: some View {
    VStack(alignment: .leading, spacing: 3) {
      Text(title)
        .font(.caption)
        .foregroundStyle(.secondary)
      Text("\(value)")
        .font(.headline.monospacedDigit())
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(.vertical, 8)
    .padding(.horizontal, 10)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }
}

private struct SearchField: View {
  @Binding var text: String

  var body: some View {
    HStack(spacing: 8) {
      Image(systemName: "magnifyingglass")
        .foregroundStyle(.secondary)
      TextField("Search VMs", text: $text)
        .textFieldStyle(.plain)
    }
    .padding(.vertical, 8)
    .padding(.horizontal, 10)
    .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
  }
}

private struct VMRowView: View {
  var virtualMachine: VirtualMachine
  var summary: DashboardVMCardSummary

  var body: some View {
    HStack(spacing: 12) {
      StatusDot(status: virtualMachine.status)

      VStack(alignment: .leading, spacing: 5) {
        Text(virtualMachine.name)
          .font(.headline)
          .lineLimit(1)
        Text(summary.subtitle)
          .font(.caption)
          .foregroundStyle(.secondary)
          .lineLimit(1)
        Text(summary.metadataItems.joined(separator: " - "))
          .font(.caption2)
          .foregroundStyle(.secondary)
          .lineLimit(1)
          .truncationMode(.middle)
      }

      Spacer(minLength: 8)

      Text(virtualMachine.status.title)
        .font(.caption)
        .foregroundStyle(.secondary)
    }
    .padding(.vertical, 6)
  }
}

struct StatusDot: View {
  var status: VirtualMachine.Status

  var body: some View {
    Circle()
      .fill(status.tint)
      .frame(width: 10, height: 10)
      .accessibilityLabel(status.title)
  }
}
