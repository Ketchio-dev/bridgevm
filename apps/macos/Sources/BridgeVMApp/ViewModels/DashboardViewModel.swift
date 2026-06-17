import Foundation
#if canImport(AppKit)
import AppKit
#endif

struct LifecycleActionOption: Identifiable, Equatable {
  var action: VirtualMachineAction
  var title: String
  var detail: String
  var systemImage: String
  var isDestructive: Bool = false

  var id: VirtualMachineAction {
    action
  }
}

@MainActor
final class DashboardViewModel: ObservableObject {
  @Published private(set) var virtualMachines: [VirtualMachine] = []
  @Published var selection: VirtualMachine.ID?
  @Published var searchText = ""
  @Published private(set) var isLoading = false
  @Published private(set) var activeActionID: VirtualMachine.ID?
  @Published private(set) var inventorySourceTitle = "Not loaded"
  @Published private(set) var lastRefreshDate: Date?
  @Published private(set) var lastRefreshError: String?
  @Published private(set) var bootTemplates: [BootTemplate] = []
  @Published private(set) var isLoadingBootTemplates = false
  @Published private(set) var modeRecommendation: ModeRecommendation?
  @Published private(set) var modeRecommendationError: String?
  @Published private(set) var isLoadingModeRecommendation = false
  @Published private(set) var readinessReports: [VirtualMachine.ID: VMReadinessReport] = [:]
  @Published private(set) var readinessReportErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var bootMediaStatuses: [VirtualMachine.ID: BootMediaStatus] = [:]
  @Published private(set) var bootMediaStatusErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var guestToolsStatuses: [VirtualMachine.ID: GuestToolsStatus] = [:]
  @Published private(set) var guestToolsStatusErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var guestToolsProvisioning: [VirtualMachine.ID: GuestToolsProvisioning] =
    [:]
  @Published private(set) var guestToolsProvisioningErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var sharedFolderLists: [VirtualMachine.ID: VMSharedFolderList] = [:]
  @Published private(set) var sharedFolderErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var lifecyclePlans: [VirtualMachine.ID: LifecyclePlan] = [:]
  @Published private(set) var lifecyclePlanErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var portForwardLists: [VirtualMachine.ID: VMPortForwardList] = [:]
  @Published private(set) var portForwardErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var openPortPlans: [VirtualMachine.ID: OpenPortPlan] = [:]
  @Published private(set) var openPortPlanErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var sshPlans: [VirtualMachine.ID: SSHPlan] = [:]
  @Published private(set) var sshPlanErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var networkPlans: [VirtualMachine.ID: NetworkPlan] = [:]
  @Published private(set) var networkPlanErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var runnerStatuses: [VirtualMachine.ID: RunnerStatus] = [:]
  @Published private(set) var runnerStatusErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var snapshotPreflightStatuses:
    [VirtualMachine.ID: SnapshotPreflightStatus] = [:]
  @Published private(set) var snapshotPreflightStatusErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var snapshots: [VirtualMachine.ID: [VMSnapshot]] = [:]
  @Published private(set) var snapshotErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var snapshotChains: [VirtualMachine.ID: VMSnapshotChain] = [:]
  @Published private(set) var snapshotChainErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var snapshotCreations: [VirtualMachine.ID: VMSnapshot] = [:]
  @Published private(set) var snapshotCreationErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var snapshotDiskCreations:
    [VirtualMachine.ID: VMSnapshotDiskCreation] = [:]
  @Published private(set) var snapshotDiskCreationErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var diskPreparations: [VirtualMachine.ID: DiskPreparation] = [:]
  @Published private(set) var diskPreparationErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var diskCreations: [VirtualMachine.ID: VMDiskCreation] = [:]
  @Published private(set) var diskCreationErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var diskInspections: [VirtualMachine.ID: VMDiskInspection] = [:]
  @Published private(set) var diskInspectionErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var diskVerifications: [VirtualMachine.ID: VMDiskVerification] = [:]
  @Published private(set) var diskVerificationErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var diskCompactions: [VirtualMachine.ID: VMDiskCompaction] = [:]
  @Published private(set) var diskCompactionErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var metadataRepairs: [VirtualMachine.ID: VMMetadataRepair] = [:]
  @Published private(set) var metadataRepairErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var manifestMigrations: [VirtualMachine.ID: VMManifestMigration] = [:]
  @Published private(set) var manifestMigrationErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var snapshotRestoreResults:
    [VirtualMachine.ID: SnapshotRestoreResult] = [:]
  @Published private(set) var snapshotRestoreErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var applicationConsistentSnapshotExecutions:
    [VirtualMachine.ID: ApplicationConsistentSnapshotExecution] = [:]
  @Published private(set) var applicationConsistentSnapshotExecutionErrors:
    [VirtualMachine.ID: String] = [:]
  @Published private(set) var diagnosticBundles: [VirtualMachine.ID: DiagnosticBundle] = [:]
  @Published private(set) var diagnosticBundleErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var vmExports: [VirtualMachine.ID: VMExportMetadata] = [:]
  @Published private(set) var vmExportErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var lastVMImport: VMImportMetadata?
  @Published private(set) var vmImportError: String?
  @Published private(set) var performanceBaselines: [VirtualMachine.ID: PerformanceBaseline] = [:]
  @Published private(set) var performanceBaselineErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var performanceSamples: [VirtualMachine.ID: PerformanceSample] = [:]
  @Published private(set) var performanceSampleErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var qmpStatuses: [VirtualMachine.ID: QMPStatus] = [:]
  @Published private(set) var qmpStatusErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var qemuLaunchPlans: [VirtualMachine.ID: QemuLaunchPlan] = [:]
  @Published private(set) var qemuLaunchPlanErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var guestToolsCommandDispatches:
    [VirtualMachine.ID: GuestToolsCommandDispatch] = [:]
  @Published private(set) var logViews: [VirtualMachine.ID: [VMLogKind: VMLogView]] = [:]
  @Published private(set) var logViewErrors: [VirtualMachine.ID: String] = [:]
  @Published private(set) var loadingReadinessReportID: VirtualMachine.ID?
  @Published private(set) var loadingBootMediaStatusID: VirtualMachine.ID?
  @Published private(set) var importingBootMediaID: VirtualMachine.ID?
  @Published private(set) var verifyingBootMediaID: VirtualMachine.ID?
  @Published private(set) var planningBootMediaDownloadID: VirtualMachine.ID?
  @Published private(set) var downloadingBootMediaID: VirtualMachine.ID?
  @Published private(set) var loadingGuestToolsStatusID: VirtualMachine.ID?
  @Published private(set) var sendingGuestToolsCommandID: VirtualMachine.ID?
  @Published private(set) var loadingSharedFoldersID: VirtualMachine.ID?
  @Published private(set) var addingSharedFolderID: VirtualMachine.ID?
  @Published private(set) var removingSharedFolderID: VirtualMachine.ID?
  @Published private(set) var loadingLifecyclePlanID: VirtualMachine.ID?
  @Published private(set) var loadingPortForwardsID: VirtualMachine.ID?
  @Published private(set) var addingPortForwardID: VirtualMachine.ID?
  @Published private(set) var removingPortForwardID: VirtualMachine.ID?
  @Published private(set) var loadingOpenPortPlanID: VirtualMachine.ID?
  @Published private(set) var loadingSSHPlanID: VirtualMachine.ID?
  @Published private(set) var loadingNetworkPlanID: VirtualMachine.ID?
  @Published private(set) var openingConsoleID: VirtualMachine.ID?
  @Published private(set) var loadingRunnerStatusID: VirtualMachine.ID?
  @Published private(set) var loadingSnapshotPreflightStatusID: VirtualMachine.ID?
  @Published private(set) var loadingSnapshotsID: VirtualMachine.ID?
  @Published private(set) var loadingSnapshotChainID: VirtualMachine.ID?
  @Published private(set) var creatingSnapshotID: VirtualMachine.ID?
  @Published private(set) var creatingSnapshotDiskID: VirtualMachine.ID?
  @Published private(set) var preparingDiskID: VirtualMachine.ID?
  @Published private(set) var creatingDiskID: VirtualMachine.ID?
  @Published private(set) var inspectingDiskID: VirtualMachine.ID?
  @Published private(set) var verifyingDiskID: VirtualMachine.ID?
  @Published private(set) var compactingDiskID: VirtualMachine.ID?
  @Published private(set) var repairingMetadataID: VirtualMachine.ID?
  @Published private(set) var checkingManifestMigrationID: VirtualMachine.ID?
  @Published private(set) var restoringSnapshotID: VirtualMachine.ID?
  @Published private(set) var executingApplicationConsistentSnapshotID: VirtualMachine.ID?
  @Published private(set) var creatingDiagnosticBundleID: VirtualMachine.ID?
  @Published private(set) var exportingVirtualMachineID: VirtualMachine.ID?
  @Published private(set) var isImportingVirtualMachine = false
  @Published private(set) var creatingPerformanceBaselineID: VirtualMachine.ID?
  @Published private(set) var creatingPerformanceSampleID: VirtualMachine.ID?
  @Published private(set) var loadingQemuLaunchPlanID: VirtualMachine.ID?
  @Published private(set) var loadingLogViewID: VirtualMachine.ID?
  @Published private(set) var isCreatingVirtualMachine = false
  @Published private(set) var cloningVirtualMachineID: VirtualMachine.ID?
  @Published private(set) var deletingVirtualMachineID: VirtualMachine.ID?
  @Published var alertMessage: String?

  private var client: VirtualMachineClient
  private let openExternalURL: @MainActor (URL) -> Bool
  private var snapshotMetadataMutationIDs: Set<VirtualMachine.ID> = []
  private var requestedModeRecommendationChoice: GuestChoice?
  private var clientGeneration = 0
  private var activeLoadGenerations: Set<Int> = []
  private var loadingBootTemplatesGeneration: Int?

  init(
    client: VirtualMachineClient,
    openExternalURL: @escaping @MainActor (URL) -> Bool = DashboardViewModel.defaultOpenExternalURL
  ) {
    self.client = client
    self.openExternalURL = openExternalURL
  }

  private static func defaultOpenExternalURL(_ url: URL) -> Bool {
    #if canImport(AppKit)
    return NSWorkspace.shared.open(url)
    #else
    return false
    #endif
  }

  func updateClient(_ client: VirtualMachineClient) {
    clientGeneration += 1
    self.client = client
    virtualMachines = []
    selection = nil
    activeActionID = nil
    activeLoadGenerations = []
    loadingBootTemplatesGeneration = nil
    isLoading = false
    inventorySourceTitle = "Not loaded"
    lastRefreshDate = nil
    lastRefreshError = nil
    bootTemplates = []
    isLoadingBootTemplates = false
    modeRecommendation = nil
    modeRecommendationError = nil
    isLoadingModeRecommendation = false
    requestedModeRecommendationChoice = nil
    readinessReports = [:]
    readinessReportErrors = [:]
    bootMediaStatuses = [:]
    bootMediaStatusErrors = [:]
    guestToolsStatuses = [:]
    guestToolsStatusErrors = [:]
    guestToolsProvisioning = [:]
    guestToolsProvisioningErrors = [:]
    sharedFolderLists = [:]
    sharedFolderErrors = [:]
    lifecyclePlans = [:]
    lifecyclePlanErrors = [:]
    portForwardLists = [:]
    portForwardErrors = [:]
    openPortPlans = [:]
    openPortPlanErrors = [:]
    sshPlans = [:]
    sshPlanErrors = [:]
    networkPlans = [:]
    networkPlanErrors = [:]
    runnerStatuses = [:]
    runnerStatusErrors = [:]
    snapshotPreflightStatuses = [:]
    snapshotPreflightStatusErrors = [:]
    snapshots = [:]
    snapshotErrors = [:]
    snapshotChains = [:]
    snapshotChainErrors = [:]
    snapshotCreations = [:]
    snapshotCreationErrors = [:]
    snapshotDiskCreations = [:]
    snapshotDiskCreationErrors = [:]
    diskPreparations = [:]
    diskPreparationErrors = [:]
    diskCreations = [:]
    diskCreationErrors = [:]
    diskInspections = [:]
    diskInspectionErrors = [:]
    diskVerifications = [:]
    diskVerificationErrors = [:]
    diskCompactions = [:]
    diskCompactionErrors = [:]
    metadataRepairs = [:]
    metadataRepairErrors = [:]
    manifestMigrations = [:]
    manifestMigrationErrors = [:]
    snapshotRestoreResults = [:]
    snapshotRestoreErrors = [:]
    applicationConsistentSnapshotExecutions = [:]
    applicationConsistentSnapshotExecutionErrors = [:]
    diagnosticBundles = [:]
    diagnosticBundleErrors = [:]
    vmExports = [:]
    vmExportErrors = [:]
    lastVMImport = nil
    vmImportError = nil
    performanceBaselines = [:]
    performanceBaselineErrors = [:]
    performanceSamples = [:]
    performanceSampleErrors = [:]
    qmpStatuses = [:]
    qmpStatusErrors = [:]
    qemuLaunchPlans = [:]
    qemuLaunchPlanErrors = [:]
    guestToolsCommandDispatches = [:]
    logViews = [:]
    logViewErrors = [:]
    loadingBootMediaStatusID = nil
    importingBootMediaID = nil
    verifyingBootMediaID = nil
    planningBootMediaDownloadID = nil
    downloadingBootMediaID = nil
    loadingGuestToolsStatusID = nil
    sendingGuestToolsCommandID = nil
    loadingSharedFoldersID = nil
    addingSharedFolderID = nil
    removingSharedFolderID = nil
    loadingLifecyclePlanID = nil
    loadingPortForwardsID = nil
    addingPortForwardID = nil
    removingPortForwardID = nil
    loadingOpenPortPlanID = nil
    loadingSSHPlanID = nil
    loadingNetworkPlanID = nil
    openingConsoleID = nil
    loadingRunnerStatusID = nil
    loadingSnapshotPreflightStatusID = nil
    loadingSnapshotsID = nil
    loadingSnapshotChainID = nil
    creatingSnapshotID = nil
    creatingSnapshotDiskID = nil
    preparingDiskID = nil
    creatingDiskID = nil
    inspectingDiskID = nil
    verifyingDiskID = nil
    compactingDiskID = nil
    repairingMetadataID = nil
    checkingManifestMigrationID = nil
    restoringSnapshotID = nil
    executingApplicationConsistentSnapshotID = nil
    creatingDiagnosticBundleID = nil
    exportingVirtualMachineID = nil
    isImportingVirtualMachine = false
    creatingPerformanceBaselineID = nil
    creatingPerformanceSampleID = nil
    loadingQemuLaunchPlanID = nil
    loadingLogViewID = nil
    loadingReadinessReportID = nil
    isCreatingVirtualMachine = false
    cloningVirtualMachineID = nil
    deletingVirtualMachineID = nil
    snapshotMetadataMutationIDs = []
    alertMessage = nil
  }

  var filteredVirtualMachines: [VirtualMachine] {
    guard !searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
      return virtualMachines
    }

    return virtualMachines.filter { vm in
      vm.name.localizedCaseInsensitiveContains(searchText)
        || vm.guest.localizedCaseInsensitiveContains(searchText)
        || vm.mode.title.localizedCaseInsensitiveContains(searchText)
    }
  }

  var selectedVirtualMachine: VirtualMachine? {
    guard let selection else {
      return nil
    }

    return virtualMachines.first(where: { $0.id == selection })
  }

  var runningCount: Int {
    virtualMachines.filter { $0.status == .running }.count
  }

  var fastModeCount: Int {
    virtualMachines.filter { $0.mode == .fast }.count
  }

  var refreshStatusText: String {
    if isLoading {
      return "Refreshing..."
    }

    if let lastRefreshError {
      return "Refresh failed: \(lastRefreshError)"
    }

    guard let lastRefreshDate else {
      return "Not refreshed yet"
    }

    return "Last refreshed \(lastRefreshDate.formatted(date: .omitted, time: .shortened))"
  }

  func load() async {
    let generation = clientGeneration
    guard !activeLoadGenerations.contains(generation) else {
      return
    }
    let loadClient = client

    activeLoadGenerations.insert(generation)
    isLoading = true
    defer {
      activeLoadGenerations.remove(generation)
      isLoading = !activeLoadGenerations.isEmpty
    }

    do {
      let loadedVirtualMachines = try await loadClient.listVirtualMachines()
      guard generation == clientGeneration else {
        return
      }
      invalidateReadinessCaches(changingFrom: virtualMachines, to: loadedVirtualMachines)
      virtualMachines = loadedVirtualMachines
      inventorySourceTitle =
        (loadClient as? VirtualMachineClientSourceProviding)?.sourceTitle ?? "Inventory"
      lastRefreshDate = Date()
      lastRefreshError = nil
      if selection == nil || selectedVirtualMachine == nil {
        selection = virtualMachines.first?.id
      }
    } catch {
      guard generation == clientGeneration else {
        return
      }
      let message = error.localizedDescription
      lastRefreshError = message
      alertMessage = message
    }
  }

  func loadBootTemplates() async {
    let generation = clientGeneration
    guard bootTemplates.isEmpty else {
      return
    }
    guard loadingBootTemplatesGeneration != generation else {
      return
    }
    let loadClient = client

    loadingBootTemplatesGeneration = generation
    isLoadingBootTemplates = true
    defer {
      if loadingBootTemplatesGeneration == generation {
        loadingBootTemplatesGeneration = nil
        isLoadingBootTemplates = false
      }
    }

    do {
      let templates = try await loadClient.listBootTemplates()
      guard generation == clientGeneration else {
        return
      }
      bootTemplates = templates
    } catch {
      guard generation == clientGeneration else {
        return
      }
      alertMessage = error.localizedDescription
    }
  }

  private var allowsMutationsForCurrentInventory: Bool {
    (client as? VirtualMachineClientSourceProviding)?.allowsMutationsForCurrentInventory ?? true
  }

  private func canMutateCurrentInventory(action: String, virtualMachine: VirtualMachine) -> Bool {
    guard allowsMutationsForCurrentInventory else {
      alertMessage =
        "\(action) blocked for \(virtualMachine.name): the current VM list came from fallback inventory. Refresh bridgevmd inventory or switch to mock inventory before changing this VM."
      return false
    }

    return true
  }

  func loadModeRecommendation(for template: BootTemplate?) async {
    await loadModeRecommendation(for: template.map(GuestChoice.init))
  }

  func loadModeRecommendation(for choice: GuestChoice?) async {
    guard let choice else {
      requestedModeRecommendationChoice = nil
      modeRecommendation = nil
      modeRecommendationError = nil
      isLoadingModeRecommendation = false
      return
    }

    requestedModeRecommendationChoice = choice
    isLoadingModeRecommendation = true
    modeRecommendationError = nil
    let generation = clientGeneration
    let recommendationClient = client

    do {
      let recommendation = try await recommendationClient.recommendMode(for: choice)
      guard generation == clientGeneration, requestedModeRecommendationChoice == choice else {
        return
      }
      modeRecommendation = recommendation
    } catch {
      guard generation == clientGeneration, requestedModeRecommendationChoice == choice else {
        return
      }
      modeRecommendation = nil
      modeRecommendationError = error.localizedDescription
    }

    if generation == clientGeneration, requestedModeRecommendationChoice == choice {
      isLoadingModeRecommendation = false
    }
  }

  func bootMediaStatus(for virtualMachine: VirtualMachine) -> BootMediaStatus? {
    bootMediaStatuses[virtualMachine.id]
  }

  func readinessReport(for virtualMachine: VirtualMachine) -> VMReadinessReport? {
    readinessReports[virtualMachine.id]
  }

  func readinessReportError(for virtualMachine: VirtualMachine) -> String? {
    readinessReportErrors[virtualMachine.id]
  }

  func bootMediaStatusError(for virtualMachine: VirtualMachine) -> String? {
    bootMediaStatusErrors[virtualMachine.id]
  }

  func guestToolsStatus(for virtualMachine: VirtualMachine) -> GuestToolsStatus? {
    guestToolsStatuses[virtualMachine.id]
  }

  func guestToolsProvisioning(for virtualMachine: VirtualMachine) -> GuestToolsProvisioning? {
    guestToolsProvisioning[virtualMachine.id]
  }

  func guestToolsProvisioningError(for virtualMachine: VirtualMachine) -> String? {
    guestToolsProvisioningErrors[virtualMachine.id]
  }

  func guestToolsStatusError(for virtualMachine: VirtualMachine) -> String? {
    guestToolsStatusErrors[virtualMachine.id]
  }

  func sharedFolderList(for virtualMachine: VirtualMachine) -> VMSharedFolderList? {
    sharedFolderLists[virtualMachine.id]
  }

  func sharedFolderError(for virtualMachine: VirtualMachine) -> String? {
    sharedFolderErrors[virtualMachine.id]
  }

  func lifecyclePlan(for virtualMachine: VirtualMachine) -> LifecyclePlan? {
    lifecyclePlans[virtualMachine.id]
  }

  func lifecyclePlanError(for virtualMachine: VirtualMachine) -> String? {
    lifecyclePlanErrors[virtualMachine.id]
  }

  func portForwardList(for virtualMachine: VirtualMachine) -> VMPortForwardList? {
    portForwardLists[virtualMachine.id]
  }

  func portForwardError(for virtualMachine: VirtualMachine) -> String? {
    portForwardErrors[virtualMachine.id]
  }

  func openPortPlan(for virtualMachine: VirtualMachine) -> OpenPortPlan? {
    openPortPlans[virtualMachine.id]
  }

  func openPortPlanError(for virtualMachine: VirtualMachine) -> String? {
    openPortPlanErrors[virtualMachine.id]
  }

  func sshPlan(for virtualMachine: VirtualMachine) -> SSHPlan? {
    sshPlans[virtualMachine.id]
  }

  func sshPlanError(for virtualMachine: VirtualMachine) -> String? {
    sshPlanErrors[virtualMachine.id]
  }

  func networkPlan(for virtualMachine: VirtualMachine) -> NetworkPlan? {
    networkPlans[virtualMachine.id]
  }

  func networkPlanError(for virtualMachine: VirtualMachine) -> String? {
    networkPlanErrors[virtualMachine.id]
  }

  func runnerStatus(for virtualMachine: VirtualMachine) -> RunnerStatus? {
    runnerStatuses[virtualMachine.id]
  }

  func runnerStatusError(for virtualMachine: VirtualMachine) -> String? {
    runnerStatusErrors[virtualMachine.id]
  }

  func snapshotPreflightStatus(for virtualMachine: VirtualMachine) -> SnapshotPreflightStatus? {
    snapshotPreflightStatuses[virtualMachine.id]
  }

  func snapshotPreflightStatusError(for virtualMachine: VirtualMachine) -> String? {
    snapshotPreflightStatusErrors[virtualMachine.id]
  }

  func snapshots(for virtualMachine: VirtualMachine) -> [VMSnapshot] {
    snapshots[virtualMachine.id] ?? []
  }

  func snapshotError(for virtualMachine: VirtualMachine) -> String? {
    snapshotErrors[virtualMachine.id]
  }

  func snapshotChain(for virtualMachine: VirtualMachine) -> VMSnapshotChain? {
    snapshotChains[virtualMachine.id]
  }

  func snapshotChainError(for virtualMachine: VirtualMachine) -> String? {
    snapshotChainErrors[virtualMachine.id]
  }

  func snapshotCreation(for virtualMachine: VirtualMachine) -> VMSnapshot? {
    snapshotCreations[virtualMachine.id]
  }

  func snapshotCreationError(for virtualMachine: VirtualMachine) -> String? {
    snapshotCreationErrors[virtualMachine.id]
  }

  func snapshotDiskCreation(for virtualMachine: VirtualMachine) -> VMSnapshotDiskCreation? {
    snapshotDiskCreations[virtualMachine.id]
  }

  func snapshotDiskCreationError(for virtualMachine: VirtualMachine) -> String? {
    snapshotDiskCreationErrors[virtualMachine.id]
  }

  func diskPreparation(for virtualMachine: VirtualMachine) -> DiskPreparation? {
    diskPreparations[virtualMachine.id]
  }

  func diskPreparationError(for virtualMachine: VirtualMachine) -> String? {
    diskPreparationErrors[virtualMachine.id]
  }

  func diskCreation(for virtualMachine: VirtualMachine) -> VMDiskCreation? {
    diskCreations[virtualMachine.id]
  }

  func diskCreationError(for virtualMachine: VirtualMachine) -> String? {
    diskCreationErrors[virtualMachine.id]
  }

  func diskInspection(for virtualMachine: VirtualMachine) -> VMDiskInspection? {
    diskInspections[virtualMachine.id]
  }

  func diskInspectionError(for virtualMachine: VirtualMachine) -> String? {
    diskInspectionErrors[virtualMachine.id]
  }

  func diskVerification(for virtualMachine: VirtualMachine) -> VMDiskVerification? {
    diskVerifications[virtualMachine.id]
  }

  func diskVerificationError(for virtualMachine: VirtualMachine) -> String? {
    diskVerificationErrors[virtualMachine.id]
  }

  func diskCompaction(for virtualMachine: VirtualMachine) -> VMDiskCompaction? {
    diskCompactions[virtualMachine.id]
  }

  func diskCompactionError(for virtualMachine: VirtualMachine) -> String? {
    diskCompactionErrors[virtualMachine.id]
  }

  func metadataRepair(for virtualMachine: VirtualMachine) -> VMMetadataRepair? {
    metadataRepairs[virtualMachine.id]
  }

  func metadataRepairError(for virtualMachine: VirtualMachine) -> String? {
    metadataRepairErrors[virtualMachine.id]
  }

  func manifestMigration(for virtualMachine: VirtualMachine) -> VMManifestMigration? {
    manifestMigrations[virtualMachine.id]
  }

  func manifestMigrationError(for virtualMachine: VirtualMachine) -> String? {
    manifestMigrationErrors[virtualMachine.id]
  }

  func snapshotRestoreResult(for virtualMachine: VirtualMachine) -> SnapshotRestoreResult? {
    snapshotRestoreResults[virtualMachine.id]
  }

  func snapshotRestoreError(for virtualMachine: VirtualMachine) -> String? {
    snapshotRestoreErrors[virtualMachine.id]
  }

  func applicationConsistentSnapshotExecution(for virtualMachine: VirtualMachine)
    -> ApplicationConsistentSnapshotExecution?
  {
    applicationConsistentSnapshotExecutions[virtualMachine.id]
  }

  func applicationConsistentSnapshotExecutionError(for virtualMachine: VirtualMachine) -> String? {
    applicationConsistentSnapshotExecutionErrors[virtualMachine.id]
  }

  func diagnosticBundle(for virtualMachine: VirtualMachine) -> DiagnosticBundle? {
    diagnosticBundles[virtualMachine.id]
  }

  func diagnosticBundleError(for virtualMachine: VirtualMachine) -> String? {
    diagnosticBundleErrors[virtualMachine.id]
  }

  func vmExport(for virtualMachine: VirtualMachine) -> VMExportMetadata? {
    vmExports[virtualMachine.id]
  }

  func vmExportError(for virtualMachine: VirtualMachine) -> String? {
    vmExportErrors[virtualMachine.id]
  }

  func performanceBaseline(for virtualMachine: VirtualMachine) -> PerformanceBaseline? {
    performanceBaselines[virtualMachine.id]
  }

  func performanceBaselineError(for virtualMachine: VirtualMachine) -> String? {
    performanceBaselineErrors[virtualMachine.id]
  }

  func performanceSample(for virtualMachine: VirtualMachine) -> PerformanceSample? {
    performanceSamples[virtualMachine.id]
  }

  func performanceSampleError(for virtualMachine: VirtualMachine) -> String? {
    performanceSampleErrors[virtualMachine.id]
  }

  func logView(kind: VMLogKind, for virtualMachine: VirtualMachine) -> VMLogView? {
    logViews[virtualMachine.id]?[kind]
  }

  func qmpStatus(for virtualMachine: VirtualMachine) -> QMPStatus? {
    qmpStatuses[virtualMachine.id]
  }

  func qmpStatusError(for virtualMachine: VirtualMachine) -> String? {
    qmpStatusErrors[virtualMachine.id]
  }

  func qemuLaunchPlan(for virtualMachine: VirtualMachine) -> QemuLaunchPlan? {
    qemuLaunchPlans[virtualMachine.id]
  }

  func qemuLaunchPlanError(for virtualMachine: VirtualMachine) -> String? {
    qemuLaunchPlanErrors[virtualMachine.id]
  }

  func guestToolsCommandDispatch(for virtualMachine: VirtualMachine) -> GuestToolsCommandDispatch?
  {
    guestToolsCommandDispatches[virtualMachine.id]
  }

  func logViewError(for virtualMachine: VirtualMachine) -> String? {
    logViewErrors[virtualMachine.id]
  }

  func cardSummary(for virtualMachine: VirtualMachine) -> DashboardVMCardSummary {
    DashboardVMCardSummary(
      virtualMachine: virtualMachine,
      readinessReport: readinessReports[virtualMachine.id],
      guestToolsStatus: guestToolsStatuses[virtualMachine.id],
      snapshots: snapshots[virtualMachine.id] ?? [],
      portForwardList: portForwardLists[virtualMachine.id]
    )
  }

  func loadBootMediaStatus(for virtualMachine: VirtualMachine) async {
    guard loadingBootMediaStatusID != virtualMachine.id else {
      return
    }

    let generation = clientGeneration
    let operationClient = client
    loadingBootMediaStatusID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        loadingBootMediaStatusID = nil
      }
    }

    do {
      let status = try await operationClient.inspectBootMediaStatus(
        on: virtualMachine.id)
      guard generation == clientGeneration else {
        return
      }
      bootMediaStatuses[virtualMachine.id] = status
      bootMediaStatusErrors[virtualMachine.id] = nil
    } catch {
      guard generation == clientGeneration else {
        return
      }
      let message = error.localizedDescription
      bootMediaStatusErrors[virtualMachine.id] = message
      alertMessage = message
    }
  }

  func loadReadinessReport(for virtualMachine: VirtualMachine) async {
    guard loadingReadinessReportID != virtualMachine.id else {
      return
    }

    let generation = clientGeneration
    let operationClient = client
    loadingReadinessReportID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        loadingReadinessReportID = nil
      }
    }

    do {
      let report = try await operationClient.inspectReadinessReport(on: virtualMachine.id)
      guard generation == clientGeneration else {
        return
      }
      readinessReports[virtualMachine.id] = report
      readinessReportErrors[virtualMachine.id] = nil
      if let bootMedia = report.bootMedia {
        bootMediaStatuses[virtualMachine.id] = bootMedia
        bootMediaStatusErrors[virtualMachine.id] = nil
      } else if let bootMediaError = report.bootMediaError {
        bootMediaStatusErrors[virtualMachine.id] = bootMediaError
      }
      if let snapshotChain = report.snapshotChain {
        snapshotChains[virtualMachine.id] = snapshotChain
        snapshotChainErrors[virtualMachine.id] = nil
      } else if let snapshotChainError = report.snapshotChainError {
        snapshotChainErrors[virtualMachine.id] = snapshotChainError
      }
      if let runner = report.runner {
        runnerStatuses[virtualMachine.id] = runner
        runnerStatusErrors[virtualMachine.id] = nil
      } else {
        runnerStatuses[virtualMachine.id] = nil
        if let runnerError = report.runnerError {
          runnerStatusErrors[virtualMachine.id] = runnerError
        } else {
          runnerStatusErrors[virtualMachine.id] = nil
        }
      }
    } catch {
      guard generation == clientGeneration else {
        return
      }
      let message = error.localizedDescription
      readinessReportErrors[virtualMachine.id] = message
      alertMessage = message
    }
  }

  func loadGuestToolsStatus(for virtualMachine: VirtualMachine) async {
    guard loadingGuestToolsStatusID != virtualMachine.id else {
      return
    }

    let generation = clientGeneration
    let operationClient = client
    loadingGuestToolsStatusID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        loadingGuestToolsStatusID = nil
      }
    }

    do {
      let status = try await operationClient.inspectGuestToolsStatus(
        on: virtualMachine.id)
      guard generation == clientGeneration else {
        return
      }
      guestToolsStatuses[virtualMachine.id] = status
      guestToolsStatusErrors[virtualMachine.id] = nil
      await loadGuestToolsProvisioning(
        for: virtualMachine, generation: generation, operationClient: operationClient)
    } catch {
      guard generation == clientGeneration else {
        return
      }
      let message = error.localizedDescription
      guestToolsStatusErrors[virtualMachine.id] = message
      alertMessage = message
    }
  }

  func loadGuestToolsProvisioning(for virtualMachine: VirtualMachine) async {
    let generation = clientGeneration
    let operationClient = client
    await loadGuestToolsProvisioning(
      for: virtualMachine, generation: generation, operationClient: operationClient)
  }

  private func loadGuestToolsProvisioning(
    for virtualMachine: VirtualMachine,
    generation: Int,
    operationClient: VirtualMachineClient
  ) async {
    do {
      async let token = operationClient.inspectGuestToolsToken(on: virtualMachine.id)
      async let deviceCommand = operationClient.inspectGuestToolsLinuxCommand(
        transport: .device,
        on: virtualMachine.id
      )
      async let socketCommand = operationClient.inspectGuestToolsLinuxCommand(
        transport: .socket,
        on: virtualMachine.id
      )
      let provisioning = try await GuestToolsProvisioning(
        token: token,
        deviceCommand: deviceCommand,
        socketCommand: socketCommand
      )
      guard generation == clientGeneration else {
        return
      }
      guestToolsProvisioning[virtualMachine.id] = provisioning
      guestToolsProvisioningErrors[virtualMachine.id] = nil
    } catch {
      guard generation == clientGeneration else {
        return
      }
      guestToolsProvisioning[virtualMachine.id] = nil
      guestToolsProvisioningErrors[virtualMachine.id] = error.localizedDescription
    }
  }

  func loadSharedFolders(for virtualMachine: VirtualMachine) async {
    guard loadingSharedFoldersID != virtualMachine.id else {
      return
    }

    let generation = clientGeneration
    let operationClient = client
    loadingSharedFoldersID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        loadingSharedFoldersID = nil
      }
    }

    do {
      let sharedFolders = try await operationClient.listSharedFolders(
        on: virtualMachine.id)
      guard generation == clientGeneration else {
        return
      }
      sharedFolderLists[virtualMachine.id] = sharedFolders
      sharedFolderErrors[virtualMachine.id] = nil
    } catch {
      guard generation == clientGeneration else {
        return
      }
      let message = error.localizedDescription
      sharedFolderErrors[virtualMachine.id] = message
      alertMessage = message
    }
  }

  func addSharedFolder(
    name nameText: String,
    hostPath hostPathText: String,
    readOnly: Bool,
    hostPathToken tokenText: String,
    for virtualMachine: VirtualMachine
  ) async -> Bool {
    guard canMutateCurrentInventory(action: "Add shared folder", virtualMachine: virtualMachine)
    else {
      return false
    }

    let name = nameText.trimmingCharacters(in: .whitespacesAndNewlines)
    let hostPath = hostPathText.trimmingCharacters(in: .whitespacesAndNewlines)
    let token = tokenText.trimmingCharacters(in: .whitespacesAndNewlines)

    guard !name.isEmpty else {
      alertMessage = "Enter a shared folder name."
      return false
    }

    guard !hostPath.isEmpty else {
      alertMessage = "Enter a host path for the shared folder."
      return false
    }

    guard addingSharedFolderID != virtualMachine.id else {
      return false
    }

    addingSharedFolderID = virtualMachine.id
    defer { addingSharedFolderID = nil }

    do {
      sharedFolderLists[virtualMachine.id] = try await client.addSharedFolder(
        named: name,
        hostPath: hostPath,
        readOnly: readOnly,
        hostPathToken: token.isEmpty ? nil : token,
        on: virtualMachine.id
      )
      sharedFolderErrors[virtualMachine.id] = nil
      await loadGuestToolsStatus(for: virtualMachine)
      alertMessage = "Shared folder '\(name)' added to the VM manifest."
      return true
    } catch {
      let message = error.localizedDescription
      sharedFolderErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func removeSharedFolder(named shareName: String, for virtualMachine: VirtualMachine) async
    -> Bool
  {
    guard canMutateCurrentInventory(action: "Remove shared folder", virtualMachine: virtualMachine)
    else {
      return false
    }

    let name = shareName.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !name.isEmpty else {
      alertMessage = "Select a shared folder to remove."
      return false
    }

    guard removingSharedFolderID != virtualMachine.id else {
      return false
    }

    removingSharedFolderID = virtualMachine.id
    defer { removingSharedFolderID = nil }

    do {
      sharedFolderLists[virtualMachine.id] = try await client.removeSharedFolder(
        named: name,
        on: virtualMachine.id
      )
      sharedFolderErrors[virtualMachine.id] = nil
      await loadGuestToolsStatus(for: virtualMachine)
      alertMessage = "Shared folder '\(name)' removed from the VM manifest."
      return true
    } catch {
      let message = error.localizedDescription
      sharedFolderErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func mountApprovedSharedFolder(named shareName: String, for virtualMachine: VirtualMachine) async
    -> Bool
  {
    guard canMutateCurrentInventory(action: "Mount shared folder", virtualMachine: virtualMachine)
    else {
      return false
    }

    let trimmedShareName = shareName.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedShareName.isEmpty else {
      alertMessage = "Select an approved shared folder to mount."
      return false
    }

    guard await canUseGuestToolsRuntimeCapability(
      "shared-folders",
      for: virtualMachine
    ) else {
      return false
    }

    guard loadingGuestToolsStatusID != virtualMachine.id else {
      return false
    }

    loadingGuestToolsStatusID = virtualMachine.id
    defer { loadingGuestToolsStatusID = nil }

    do {
      if let status = try await client.mountApprovedSharedFolder(
        named: trimmedShareName,
        on: virtualMachine.id
      ) {
        guestToolsStatuses[virtualMachine.id] = status
      }
      guestToolsStatusErrors[virtualMachine.id] = nil
      return true
    } catch {
      let message = error.localizedDescription
      guestToolsStatusErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func unmountApprovedSharedFolder(named shareName: String, for virtualMachine: VirtualMachine)
    async -> Bool
  {
    guard canMutateCurrentInventory(action: "Unmount shared folder", virtualMachine: virtualMachine)
    else {
      return false
    }

    let trimmedShareName = shareName.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedShareName.isEmpty else {
      alertMessage = "Select a shared folder entry to unmount."
      return false
    }

    guard await canUseGuestToolsRuntimeCapability(
      "shared-folders",
      for: virtualMachine
    ) else {
      return false
    }

    guard loadingGuestToolsStatusID != virtualMachine.id else {
      return false
    }

    loadingGuestToolsStatusID = virtualMachine.id
    defer { loadingGuestToolsStatusID = nil }

    do {
      if let status = try await client.unmountApprovedSharedFolder(
        named: trimmedShareName,
        on: virtualMachine.id
      ) {
        guestToolsStatuses[virtualMachine.id] = status
      }
      guestToolsStatusErrors[virtualMachine.id] = nil
      return true
    } catch {
      let message = error.localizedDescription
      guestToolsStatusErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func sendGuestToolsCommand(
    _ command: GuestToolsAgentCommand,
    requestID: String? = nil,
    for virtualMachine: VirtualMachine
  ) async -> Bool {
    guard canMutateCurrentInventory(action: "Send guest tools command", virtualMachine: virtualMachine)
    else {
      return false
    }

    guard await canSendGuestToolsCommand(command, for: virtualMachine) else {
      return false
    }

    guard sendingGuestToolsCommandID != virtualMachine.id else {
      return false
    }

    sendingGuestToolsCommandID = virtualMachine.id
    defer { sendingGuestToolsCommandID = nil }

    let effectiveRequestID = requestID ?? makeGuestToolsRequestID(for: command)

    do {
      let dispatch = try await client.sendGuestToolsCommand(
        command,
        requestID: effectiveRequestID,
        on: virtualMachine.id
      )
      guestToolsCommandDispatches[virtualMachine.id] = dispatch
      guestToolsStatusErrors[virtualMachine.id] = nil
      await loadGuestToolsStatus(for: virtualMachine)
      return true
    } catch {
      let message = error.localizedDescription
      guestToolsStatusErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  private func canSendGuestToolsCommand(
    _ command: GuestToolsAgentCommand,
    for virtualMachine: VirtualMachine
  ) async -> Bool {
    await canUseGuestToolsRuntimeCapability(
      command.requiredRuntimeCapability,
      for: virtualMachine
    )
  }

  private func canUseGuestToolsRuntimeCapability(
    _ capability: String,
    for virtualMachine: VirtualMachine
  ) async -> Bool {
    if guestToolsStatuses[virtualMachine.id] == nil {
      await loadGuestToolsStatus(for: virtualMachine)
    }

    guard let status = guestToolsStatuses[virtualMachine.id] else {
      let detail = guestToolsStatusErrors[virtualMachine.id] ?? "guest tools status is unavailable"
      alertMessage = "Guest tools command blocked for \(virtualMachine.name): \(detail)."
      return false
    }

    guard status.connected else {
      alertMessage = "Guest tools command blocked for \(virtualMachine.name): guest agent is not connected."
      return false
    }

    guard status.runtime?.capabilities.contains(capability) == true else {
      alertMessage =
        "Guest tools command blocked for \(virtualMachine.name): runtime capability \(capability) is not advertised."
      return false
    }

    return true
  }

  func sendClipboardText(_ text: String, for virtualMachine: VirtualMachine) async -> Bool {
    let trimmedText = text.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedText.isEmpty else {
      alertMessage = "Enter clipboard text to send."
      return false
    }

    return await sendGuestToolsCommand(
      .setClipboard(text: trimmedText),
      for: virtualMachine
    )
  }

  func resizeDisplay(
    width widthText: String,
    height heightText: String,
    scale scaleText: String,
    for virtualMachine: VirtualMachine
  ) async -> Bool {
    let trimmedWidth = widthText.trimmingCharacters(in: .whitespacesAndNewlines)
    let trimmedHeight = heightText.trimmingCharacters(in: .whitespacesAndNewlines)
    let trimmedScale = scaleText.trimmingCharacters(in: .whitespacesAndNewlines)

    guard let width = UInt32(trimmedWidth), width > 0 else {
      alertMessage = "Enter a valid display width."
      return false
    }
    guard let height = UInt32(trimmedHeight), height > 0 else {
      alertMessage = "Enter a valid display height."
      return false
    }
    guard let scale = UInt16(trimmedScale), scale > 0 else {
      alertMessage = "Enter a valid display scale."
      return false
    }

    return await sendGuestToolsCommand(
      .resizeDisplay(width: width, height: height, scale: scale),
      for: virtualMachine
    )
  }

  func syncGuestTime(for virtualMachine: VirtualMachine) async -> Bool {
    let millis = UInt64(Date().timeIntervalSince1970 * 1_000)
    return await sendGuestToolsCommand(
      .timeSync(unixEpochMillis: millis),
      for: virtualMachine
    )
  }

  func launchApplication(id applicationID: String, for virtualMachine: VirtualMachine) async -> Bool {
    let trimmedID = applicationID.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedID.isEmpty else {
      alertMessage = "Enter an application ID to launch."
      return false
    }

    return await sendGuestToolsCommand(
      .launchApplication(id: trimmedID),
      for: virtualMachine
    )
  }

  func focusWindow(id windowID: String, for virtualMachine: VirtualMachine) async -> Bool {
    let trimmedID = windowID.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedID.isEmpty else {
      alertMessage = "Enter a window ID to focus."
      return false
    }

    return await sendGuestToolsCommand(
      .focusWindow(id: trimmedID),
      for: virtualMachine
    )
  }

  func closeWindow(id windowID: String, for virtualMachine: VirtualMachine) async -> Bool {
    let trimmedID = windowID.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedID.isEmpty else {
      alertMessage = "Enter a window ID to close."
      return false
    }

    return await sendGuestToolsCommand(
      .closeWindow(id: trimmedID),
      for: virtualMachine
    )
  }

  func sendInlineFileDrop(
    fileName: String,
    contents: String,
    for virtualMachine: VirtualMachine
  ) async -> Bool {
    let trimmedFileName = fileName.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedFileName.isEmpty else {
      alertMessage = "Enter a file name to drop."
      return false
    }

    let trimmedContents = contents.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedContents.isEmpty else {
      alertMessage = "Enter file contents to drop."
      return false
    }

    let payload = Data(trimmedContents.utf8)
    let transferID = "drop-\(UUID().uuidString.lowercased())"
    let start = await sendGuestToolsCommand(
      .fileDropStart(
        transferID: transferID,
        fileName: trimmedFileName,
        sizeBytes: UInt64(payload.count)
      ),
      requestID: "\(transferID)-start",
      for: virtualMachine
    )
    guard start else {
      return false
    }

    let chunk = await sendGuestToolsCommand(
      .fileDropChunk(
        transferID: transferID,
        chunkIndex: 0,
        dataBase64: payload.base64EncodedString()
      ),
      requestID: "\(transferID)-chunk-0",
      for: virtualMachine
    )
    guard chunk else {
      return false
    }

    return await sendGuestToolsCommand(
      .fileDropComplete(transferID: transferID),
      requestID: "\(transferID)-complete",
      for: virtualMachine
    )
  }

  func loadRunnerStatus(for virtualMachine: VirtualMachine) async {
    guard loadingRunnerStatusID != virtualMachine.id else {
      return
    }

    let generation = clientGeneration
    let operationClient = client
    loadingRunnerStatusID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        loadingRunnerStatusID = nil
      }
    }

    do {
      if let status = try await operationClient.inspectRunnerStatus(on: virtualMachine.id) {
        guard generation == clientGeneration else {
          return
        }
        runnerStatuses[virtualMachine.id] = status
      } else {
        guard generation == clientGeneration else {
          return
        }
        runnerStatuses[virtualMachine.id] = nil
      }
      runnerStatusErrors[virtualMachine.id] = nil
    } catch {
      guard generation == clientGeneration else {
        return
      }
      let message = error.localizedDescription
      runnerStatusErrors[virtualMachine.id] = message
      alertMessage = message
    }
  }

  func prepareRun(for virtualMachine: VirtualMachine) async -> Bool {
    guard canMutateCurrentInventory(action: "Prepare run", virtualMachine: virtualMachine) else {
      return false
    }

    guard loadingRunnerStatusID != virtualMachine.id else {
      return false
    }

    let generation = clientGeneration
    let operationClient = client
    loadingRunnerStatusID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        loadingRunnerStatusID = nil
      }
    }

    do {
      let status = try await operationClient.prepareRun(on: virtualMachine.id)
      guard generation == clientGeneration else {
        return false
      }
      runnerStatuses[virtualMachine.id] = status
      runnerStatusErrors[virtualMachine.id] = nil
      alertMessage = "\(virtualMachine.name) launch readiness prepared."
      return true
    } catch {
      guard generation == clientGeneration else {
        return false
      }
      let message = error.localizedDescription
      runnerStatusErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func loadQemuLaunchPlan(for virtualMachine: VirtualMachine) async {
    guard loadingQemuLaunchPlanID != virtualMachine.id else {
      return
    }

    let generation = clientGeneration
    let operationClient = client
    loadingQemuLaunchPlanID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        loadingQemuLaunchPlanID = nil
      }
    }

    do {
      let plan = try await operationClient.inspectQemuArgs(on: virtualMachine.id)
      guard generation == clientGeneration else {
        return
      }
      qemuLaunchPlans[virtualMachine.id] = plan
      qemuLaunchPlanErrors[virtualMachine.id] = nil
    } catch {
      guard generation == clientGeneration else {
        return
      }
      let message = error.localizedDescription
      qemuLaunchPlanErrors[virtualMachine.id] = message
      alertMessage = message
    }
  }

  func loadSnapshotPreflightStatus(for virtualMachine: VirtualMachine) async {
    guard loadingSnapshotPreflightStatusID != virtualMachine.id else {
      return
    }

    let generation = clientGeneration
    let operationClient = client
    loadingSnapshotPreflightStatusID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        loadingSnapshotPreflightStatusID = nil
      }
    }

    do {
      let status = try await operationClient.inspectSnapshotPreflightStatus(on: virtualMachine.id)
      guard generation == clientGeneration else {
        return
      }
      snapshotPreflightStatuses[virtualMachine.id] = status
      snapshotPreflightStatusErrors[virtualMachine.id] = nil
    } catch {
      guard generation == clientGeneration else {
        return
      }
      let message = error.localizedDescription
      snapshotPreflightStatusErrors[virtualMachine.id] = message
      alertMessage = message
    }
  }

  func loadSnapshots(for virtualMachine: VirtualMachine) async {
    guard loadingSnapshotsID != virtualMachine.id else {
      return
    }

    let generation = clientGeneration
    let operationClient = client
    loadingSnapshotsID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        loadingSnapshotsID = nil
      }
    }

    do {
      let snapshotList = try await operationClient.listSnapshots(on: virtualMachine.id)
      guard generation == clientGeneration else {
        return
      }
      snapshots[virtualMachine.id] = snapshotList
      snapshotErrors[virtualMachine.id] = nil
    } catch {
      guard generation == clientGeneration else {
        return
      }
      let message = error.localizedDescription
      snapshotErrors[virtualMachine.id] = message
      alertMessage = message
    }
  }

  func loadSnapshotChain(for virtualMachine: VirtualMachine) async {
    guard loadingSnapshotChainID != virtualMachine.id else {
      return
    }

    let generation = clientGeneration
    let operationClient = client
    loadingSnapshotChainID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        loadingSnapshotChainID = nil
      }
    }

    do {
      let chain = try await operationClient.inspectSnapshotChain(
        on: virtualMachine.id)
      guard generation == clientGeneration else {
        return
      }
      snapshotChains[virtualMachine.id] = chain
      snapshotChainErrors[virtualMachine.id] = nil
    } catch {
      guard generation == clientGeneration else {
        return
      }
      let message = error.localizedDescription
      snapshotChainErrors[virtualMachine.id] = message
      alertMessage = message
    }
  }

  func preparePrimaryDisk(for virtualMachine: VirtualMachine) async -> Bool {
    guard canMutateCurrentInventory(action: "Prepare disk", virtualMachine: virtualMachine) else {
      return false
    }

    guard preparingDiskID != virtualMachine.id else {
      return false
    }

    let generation = clientGeneration
    let operationClient = client
    preparingDiskID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        preparingDiskID = nil
      }
    }

    do {
      let preparation = try await operationClient.preparePrimaryDisk(on: virtualMachine.id)
      guard generation == clientGeneration else {
        return false
      }
      diskPreparations[virtualMachine.id] = preparation
      diskPreparationErrors[virtualMachine.id] = nil
      clearReadinessCaches(for: virtualMachine.id)
      alertMessage = "Primary disk metadata prepared for \(preparation.path)."
      return true
    } catch {
      guard generation == clientGeneration else {
        return false
      }
      let message = error.localizedDescription
      diskPreparationErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func createPrimaryDisk(for virtualMachine: VirtualMachine) async -> Bool {
    guard canMutateCurrentInventory(action: "Create disk", virtualMachine: virtualMachine) else {
      return false
    }

    guard creatingDiskID != virtualMachine.id else {
      return false
    }

    let generation = clientGeneration
    let operationClient = client
    creatingDiskID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        creatingDiskID = nil
      }
    }

    do {
      let creation = try await operationClient.createPrimaryDisk(on: virtualMachine.id)
      guard generation == clientGeneration else {
        return false
      }
      diskCreations[virtualMachine.id] = creation
      diskCreationErrors[virtualMachine.id] = nil
      diskPreparations[virtualMachine.id] = creation.preparation
      clearReadinessCaches(for: virtualMachine.id)
      alertMessage =
        creation.executed
        ? "Primary disk create command finished with \(creation.exitStatus ?? "unknown status")."
        : "Primary disk was already ready at \(creation.preparation.path)."
      return true
    } catch {
      guard generation == clientGeneration else {
        return false
      }
      let message = error.localizedDescription
      diskCreationErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func inspectPrimaryDisk(for virtualMachine: VirtualMachine) async -> Bool {
    guard inspectingDiskID != virtualMachine.id else {
      return false
    }

    let generation = clientGeneration
    let operationClient = client
    inspectingDiskID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        inspectingDiskID = nil
      }
    }

    do {
      let inspection = try await operationClient.inspectPrimaryDisk(on: virtualMachine.id)
      guard generation == clientGeneration else {
        return false
      }
      diskInspections[virtualMachine.id] = inspection
      diskInspectionErrors[virtualMachine.id] = nil
      diskPreparations[virtualMachine.id] = inspection.preparation
      alertMessage = "Primary disk inspected with status \(inspection.exitStatus)."
      return true
    } catch {
      guard generation == clientGeneration else {
        return false
      }
      let message = error.localizedDescription
      diskInspectionErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func verifyActiveDisk(for virtualMachine: VirtualMachine) async -> Bool {
    guard canMutateCurrentInventory(action: "Verify disk", virtualMachine: virtualMachine) else {
      return false
    }

    guard verifyingDiskID != virtualMachine.id else {
      return false
    }

    let generation = clientGeneration
    let operationClient = client
    verifyingDiskID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        verifyingDiskID = nil
      }
    }

    do {
      let verification = try await operationClient.verifyActiveDisk(on: virtualMachine.id)
      guard generation == clientGeneration else {
        return false
      }
      diskVerifications[virtualMachine.id] = verification
      diskVerificationErrors[virtualMachine.id] = nil
      clearReadinessCaches(for: virtualMachine.id)
      alertMessage = "Active disk verified with status \(verification.exitStatus)."
      return true
    } catch {
      guard generation == clientGeneration else {
        return false
      }
      let message = error.localizedDescription
      diskVerificationErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func compactActiveDisk(for virtualMachine: VirtualMachine) async -> Bool {
    guard canMutateCurrentInventory(action: "Compact disk", virtualMachine: virtualMachine) else {
      return false
    }

    guard compactingDiskID != virtualMachine.id else {
      return false
    }

    let generation = clientGeneration
    let operationClient = client
    compactingDiskID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        compactingDiskID = nil
      }
    }

    do {
      let compaction = try await operationClient.compactActiveDisk(on: virtualMachine.id)
      guard generation == clientGeneration else {
        return false
      }
      diskCompactions[virtualMachine.id] = compaction
      diskCompactionErrors[virtualMachine.id] = nil
      clearReadinessCaches(for: virtualMachine.id)
      await loadSnapshotChain(for: virtualMachine)
      guard generation == clientGeneration else {
        return false
      }
      alertMessage = "Active disk compacted; previous image kept at \(compaction.backupPath)."
      return true
    } catch {
      guard generation == clientGeneration else {
        return false
      }
      let message = error.localizedDescription
      diskCompactionErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func repairMetadata(for virtualMachine: VirtualMachine) async -> Bool {
    guard canMutateCurrentInventory(action: "Repair metadata", virtualMachine: virtualMachine) else {
      return false
    }

    guard repairingMetadataID != virtualMachine.id else {
      return false
    }

    repairingMetadataID = virtualMachine.id
    defer { repairingMetadataID = nil }

    do {
      let repair = try await client.repairMetadata(on: virtualMachine.id)
      metadataRepairs[virtualMachine.id] = repair
      metadataRepairErrors[virtualMachine.id] = nil
      clearReadinessCaches(for: virtualMachine.id)
      await loadSnapshots(for: virtualMachine)
      await loadSnapshotChain(for: virtualMachine)
      alertMessage =
        repair.repaired
        ? "Metadata repaired with \(repair.actions.count) action(s)."
        : "Metadata repair completed; no metadata repairs were needed."
      return true
    } catch {
      let message = error.localizedDescription
      metadataRepairErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func checkManifestMigration(for virtualMachine: VirtualMachine) async -> Bool {
    guard canMutateCurrentInventory(action: "Check manifest migration", virtualMachine: virtualMachine)
    else {
      return false
    }

    guard checkingManifestMigrationID != virtualMachine.id else {
      return false
    }

    checkingManifestMigrationID = virtualMachine.id
    defer { checkingManifestMigrationID = nil }

    do {
      let migration = try await client.migrateManifest(on: virtualMachine.id, dryRun: true)
      manifestMigrations[virtualMachine.id] = migration
      manifestMigrationErrors[virtualMachine.id] = nil
      clearReadinessCaches(for: virtualMachine.id)
      alertMessage =
        migration.migrated
        ? "Manifest migration dry run found \(migration.actions.count) action(s)."
        : "Manifest migration dry run completed; manifest is current."
      return true
    } catch {
      let message = error.localizedDescription
      manifestMigrationErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func createSnapshotDisk(named snapshotName: String, for virtualMachine: VirtualMachine) async
    -> Bool
  {
    guard canMutateCurrentInventory(action: "Create snapshot disk", virtualMachine: virtualMachine)
    else {
      return false
    }

    let trimmedSnapshotName = snapshotName.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedSnapshotName.isEmpty else {
      alertMessage = "Enter a snapshot name before creating a disk overlay."
      return false
    }

    guard creatingSnapshotDiskID != virtualMachine.id else {
      return false
    }
    guard beginSnapshotMetadataMutation(for: virtualMachine) else {
      return false
    }

    creatingSnapshotDiskID = virtualMachine.id
    defer {
      creatingSnapshotDiskID = nil
      endSnapshotMetadataMutation(for: virtualMachine)
    }

    do {
      let creation = try await client.createSnapshotDisk(
        named: trimmedSnapshotName,
        on: virtualMachine.id
      )
      snapshotDiskCreations[virtualMachine.id] = creation
      snapshotDiskCreationErrors[virtualMachine.id] = nil
      clearReadinessCaches(for: virtualMachine.id)
      await loadSnapshotChain(for: virtualMachine)
      alertMessage =
        creation.executed
        ? "Snapshot disk '\(creation.snapshot)' create command finished with \(creation.exitStatus ?? "unknown status")."
        : "Snapshot disk '\(creation.snapshot)' overlay was already ready at \(creation.disk.overlayPath)."
      return true
    } catch {
      let message = error.localizedDescription
      snapshotDiskCreationErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func createSnapshot(
    named snapshotName: String,
    kind: VMSnapshotKind,
    for virtualMachine: VirtualMachine
  ) async -> Bool {
    guard canMutateCurrentInventory(action: "Create snapshot", virtualMachine: virtualMachine)
    else {
      return false
    }

    let trimmedSnapshotName = snapshotName.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedSnapshotName.isEmpty else {
      alertMessage = "Enter a snapshot name before creating metadata."
      return false
    }

    guard creatingSnapshotID != virtualMachine.id else {
      return false
    }
    guard beginSnapshotMetadataMutation(for: virtualMachine) else {
      return false
    }

    creatingSnapshotID = virtualMachine.id
    defer {
      creatingSnapshotID = nil
      endSnapshotMetadataMutation(for: virtualMachine)
    }

    do {
      let snapshot = try await client.createSnapshot(
        named: trimmedSnapshotName,
        kind: kind,
        on: virtualMachine.id
      )
      snapshotCreations[virtualMachine.id] = snapshot
      snapshotCreationErrors[virtualMachine.id] = nil
      clearReadinessCaches(for: virtualMachine.id)
      await loadSnapshots(for: virtualMachine)
      await loadSnapshotChain(for: virtualMachine)
      alertMessage = "Snapshot '\(snapshot.name)' \(snapshot.kind.title.lowercased()) metadata created."
      return true
    } catch {
      let message = error.localizedDescription
      snapshotCreationErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func restoreSnapshot(named snapshotName: String, for virtualMachine: VirtualMachine) async
    -> Bool
  {
    guard canMutateCurrentInventory(action: "Restore snapshot", virtualMachine: virtualMachine)
    else {
      return false
    }

    let trimmedSnapshotName = snapshotName.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedSnapshotName.isEmpty else {
      alertMessage = "Select a snapshot to restore."
      return false
    }

    guard restoringSnapshotID != virtualMachine.id else {
      return false
    }
    guard beginSnapshotMetadataMutation(for: virtualMachine) else {
      return false
    }

    restoringSnapshotID = virtualMachine.id
    defer {
      restoringSnapshotID = nil
      endSnapshotMetadataMutation(for: virtualMachine)
    }

    do {
      let result = try await client.restoreSnapshot(
        named: trimmedSnapshotName,
        on: virtualMachine.id
      )
      snapshotRestoreResults[virtualMachine.id] = result
      snapshotRestoreErrors[virtualMachine.id] = nil
      clearReadinessCaches(for: virtualMachine.id)
      await loadSnapshots(for: virtualMachine)
      await loadSnapshotChain(for: virtualMachine)
      alertMessage = "Snapshot '\(result.snapshot)' metadata restored."
      return true
    } catch {
      let message = error.localizedDescription
      snapshotRestoreErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func loadLifecyclePlan(
    action: LifecyclePlanAction,
    for virtualMachine: VirtualMachine
  ) async {
    guard loadingLifecyclePlanID != virtualMachine.id else {
      return
    }

    let generation = clientGeneration
    let operationClient = client
    loadingLifecyclePlanID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        loadingLifecyclePlanID = nil
      }
    }

    do {
      let plan = try await operationClient.inspectLifecyclePlan(
        action: action,
        on: virtualMachine.id
      )
      guard generation == clientGeneration else {
        return
      }
      lifecyclePlans[virtualMachine.id] = plan
      lifecyclePlanErrors[virtualMachine.id] = nil
    } catch {
      guard generation == clientGeneration else {
        return
      }
      let message = error.localizedDescription
      lifecyclePlanErrors[virtualMachine.id] = message
      alertMessage = message
    }
  }

  func loadOpenPortPlan(
    guestPort guestPortText: String,
    scheme schemeText: String,
    for virtualMachine: VirtualMachine
  ) async -> Bool {
    let trimmedGuestPort = guestPortText.trimmingCharacters(in: .whitespacesAndNewlines)
    let trimmedScheme = schemeText.trimmingCharacters(in: .whitespacesAndNewlines)
    let scheme = trimmedScheme.isEmpty ? "http" : trimmedScheme

    guard let guestPort = UInt16(trimmedGuestPort), guestPort > 0 else {
      alertMessage = "Enter a valid guest port."
      return false
    }

    guard loadingOpenPortPlanID != virtualMachine.id else {
      return false
    }

    let generation = clientGeneration
    let operationClient = client
    loadingOpenPortPlanID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        loadingOpenPortPlanID = nil
      }
    }

    do {
      let plan = try await operationClient.inspectOpenPortPlan(
        guestPort: guestPort,
        scheme: scheme,
        on: virtualMachine.id
      )
      guard generation == clientGeneration else {
        return false
      }
      openPortPlans[virtualMachine.id] = plan
      openPortPlanErrors[virtualMachine.id] = nil
      return true
    } catch {
      guard generation == clientGeneration else {
        return false
      }
      let message = error.localizedDescription
      openPortPlanErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func loadSSHPlan(user userText: String, for virtualMachine: VirtualMachine) async -> Bool {
    let user = userText.trimmingCharacters(in: .whitespacesAndNewlines)

    guard !user.isEmpty else {
      alertMessage = "Enter an SSH user."
      return false
    }

    guard loadingSSHPlanID != virtualMachine.id else {
      return false
    }

    let generation = clientGeneration
    let operationClient = client
    loadingSSHPlanID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        loadingSSHPlanID = nil
      }
    }

    do {
      let plan = try await operationClient.inspectSSHPlan(
        user: user,
        on: virtualMachine.id
      )
      guard generation == clientGeneration else {
        return false
      }
      sshPlans[virtualMachine.id] = plan
      sshPlanErrors[virtualMachine.id] = nil
      return true
    } catch {
      guard generation == clientGeneration else {
        return false
      }
      let message = error.localizedDescription
      sshPlanErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func loadNetworkPlan(for virtualMachine: VirtualMachine) async {
    guard loadingNetworkPlanID != virtualMachine.id else {
      return
    }

    let generation = clientGeneration
    let operationClient = client
    loadingNetworkPlanID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        loadingNetworkPlanID = nil
      }
    }

    do {
      let plan = try await operationClient.inspectNetworkPlan(on: virtualMachine.id)
      guard generation == clientGeneration else {
        return
      }
      networkPlans[virtualMachine.id] = plan
      networkPlanErrors[virtualMachine.id] = nil
    } catch {
      guard generation == clientGeneration else {
        return
      }
      let message = error.localizedDescription
      networkPlanErrors[virtualMachine.id] = message
      alertMessage = message
    }
  }

  func loadPortForwards(for virtualMachine: VirtualMachine) async {
    guard loadingPortForwardsID != virtualMachine.id else {
      return
    }

    let generation = clientGeneration
    let operationClient = client
    loadingPortForwardsID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        loadingPortForwardsID = nil
      }
    }

    do {
      let forwards = try await operationClient.listPortForwards(on: virtualMachine.id)
      guard generation == clientGeneration else {
        return
      }
      portForwardLists[virtualMachine.id] = forwards
      portForwardErrors[virtualMachine.id] = nil
    } catch {
      guard generation == clientGeneration else {
        return
      }
      let message = error.localizedDescription
      portForwardErrors[virtualMachine.id] = message
      alertMessage = message
    }
  }

  func addPortForward(
    host hostText: String,
    guest guestText: String,
    for virtualMachine: VirtualMachine
  ) async -> Bool {
    guard canMutateCurrentInventory(action: "Add port forward", virtualMachine: virtualMachine)
    else {
      return false
    }

    let trimmedHost = hostText.trimmingCharacters(in: .whitespacesAndNewlines)
    let trimmedGuest = guestText.trimmingCharacters(in: .whitespacesAndNewlines)
    guard let host = UInt16(trimmedHost), host > 0,
      let guest = UInt16(trimmedGuest), guest > 0
    else {
      alertMessage = "Enter valid host and guest ports from 1 to 65535."
      return false
    }

    guard addingPortForwardID != virtualMachine.id else {
      return false
    }

    addingPortForwardID = virtualMachine.id
    defer { addingPortForwardID = nil }

    do {
      portForwardLists[virtualMachine.id] = try await client.addPortForward(
        host: host,
        guest: guest,
        on: virtualMachine.id
      )
      portForwardErrors[virtualMachine.id] = nil
      openPortPlans[virtualMachine.id] = nil
      clearReadinessCaches(for: virtualMachine.id)
      alertMessage = "Port forward \(host):\(guest) added to the VM manifest."
      return true
    } catch {
      let message = error.localizedDescription
      portForwardErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func removePortForward(host: UInt16, guest: UInt16, for virtualMachine: VirtualMachine) async
    -> Bool
  {
    guard canMutateCurrentInventory(action: "Remove port forward", virtualMachine: virtualMachine)
    else {
      return false
    }

    guard removingPortForwardID != virtualMachine.id else {
      return false
    }

    removingPortForwardID = virtualMachine.id
    defer { removingPortForwardID = nil }

    do {
      portForwardLists[virtualMachine.id] = try await client.removePortForward(
        host: host,
        guest: guest,
        on: virtualMachine.id
      )
      portForwardErrors[virtualMachine.id] = nil
      openPortPlans[virtualMachine.id] = nil
      clearReadinessCaches(for: virtualMachine.id)
      alertMessage = "Port forward \(host):\(guest) removed from the VM manifest."
      return true
    } catch {
      let message = error.localizedDescription
      portForwardErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func executeApplicationConsistentSnapshot(
    named snapshotName: String,
    freezeTimeoutMillis: UInt64? = nil,
    for virtualMachine: VirtualMachine
  ) async -> Bool {
    guard canMutateCurrentInventory(
      action: "Execute application-consistent snapshot",
      virtualMachine: virtualMachine
    ) else {
      return false
    }

    let trimmedSnapshotName = snapshotName.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedSnapshotName.isEmpty else {
      alertMessage = "Enter a snapshot name before executing application-consistent snapshot."
      return false
    }

    guard executingApplicationConsistentSnapshotID != virtualMachine.id else {
      return false
    }
    guard beginSnapshotMetadataMutation(for: virtualMachine) else {
      return false
    }

    executingApplicationConsistentSnapshotID = virtualMachine.id
    defer {
      executingApplicationConsistentSnapshotID = nil
      endSnapshotMetadataMutation(for: virtualMachine)
    }

    do {
      let execution = try await client.executeApplicationConsistentSnapshot(
        named: trimmedSnapshotName,
        freezeTimeoutMillis: freezeTimeoutMillis,
        on: virtualMachine.id
      )
      applicationConsistentSnapshotExecutions[virtualMachine.id] = execution
      applicationConsistentSnapshotExecutionErrors[virtualMachine.id] = nil
      snapshotPreflightStatusErrors[virtualMachine.id] = nil
      alertMessage =
        "Application-consistent snapshot '\(execution.snapshot)' executed for \(execution.vm)."
      return true
    } catch {
      let message = error.localizedDescription
      applicationConsistentSnapshotExecutionErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func createDiagnosticBundle(output outputText: String, for virtualMachine: VirtualMachine) async
    -> Bool
  {
    guard canMutateCurrentInventory(action: "Create diagnostic bundle", virtualMachine: virtualMachine)
    else {
      return false
    }

    guard let output = requiredMetadataOutput(outputText) else {
      let message = VirtualMachineClientError.outputPathRequired.localizedDescription
      diagnosticBundleErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }

    guard creatingDiagnosticBundleID != virtualMachine.id else {
      return false
    }

    let generation = clientGeneration
    let operationClient = client
    creatingDiagnosticBundleID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        creatingDiagnosticBundleID = nil
      }
    }

    do {
      let bundle = try await operationClient.createDiagnosticBundle(
        output: output,
        on: virtualMachine.id
      )
      guard generation == clientGeneration else {
        return false
      }
      diagnosticBundles[virtualMachine.id] = bundle
      diagnosticBundleErrors[virtualMachine.id] = nil
      alertMessage = "Diagnostic bundle created at \(bundle.output)."
      return true
    } catch {
      guard generation == clientGeneration else {
        return false
      }
      let message = error.localizedDescription
      diagnosticBundleErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func createPerformanceBaseline(output outputText: String, for virtualMachine: VirtualMachine)
    async -> Bool
  {
    guard canMutateCurrentInventory(action: "Create performance baseline", virtualMachine: virtualMachine)
    else {
      return false
    }

    guard let output = requiredMetadataOutput(outputText) else {
      let message = VirtualMachineClientError.outputPathRequired.localizedDescription
      performanceBaselineErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }

    guard creatingPerformanceBaselineID != virtualMachine.id else {
      return false
    }

    let generation = clientGeneration
    let operationClient = client
    creatingPerformanceBaselineID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        creatingPerformanceBaselineID = nil
      }
    }

    do {
      let baseline = try await operationClient.createPerformanceBaseline(
        output: output,
        on: virtualMachine.id
      )
      guard generation == clientGeneration else {
        return false
      }
      performanceBaselines[virtualMachine.id] = baseline
      performanceBaselineErrors[virtualMachine.id] = nil
      alertMessage = "Performance baseline metadata created at \(baseline.artifact)."
      return true
    } catch {
      guard generation == clientGeneration else {
        return false
      }
      let message = error.localizedDescription
      performanceBaselineErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func createPerformanceSample(
    output outputText: String,
    artifactBytes artifactBytesText: String,
    iterations iterationsText: String,
    sync: Bool,
    for virtualMachine: VirtualMachine
  ) async -> Bool {
    guard canMutateCurrentInventory(action: "Create performance sample", virtualMachine: virtualMachine)
    else {
      return false
    }

    guard let output = requiredMetadataOutput(outputText) else {
      let message = VirtualMachineClientError.outputPathRequired.localizedDescription
      performanceSampleErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }

    let trimmedArtifactBytes = artifactBytesText.trimmingCharacters(in: .whitespacesAndNewlines)
    let trimmedIterations = iterationsText.trimmingCharacters(in: .whitespacesAndNewlines)

    guard let artifactBytes = UInt64(trimmedArtifactBytes), artifactBytes > 0 else {
      alertMessage = "Enter a valid performance probe byte count."
      return false
    }

    guard artifactBytes <= 64 * 1024 * 1024 else {
      alertMessage = "Performance probe byte count must be 64 MiB or smaller."
      return false
    }

    guard let iterations = UInt16(trimmedIterations), iterations > 0 else {
      alertMessage = "Enter a valid performance sample iteration count."
      return false
    }

    guard iterations <= 16 else {
      alertMessage = "Performance sample iterations must be 16 or fewer."
      return false
    }

    guard creatingPerformanceSampleID != virtualMachine.id else {
      return false
    }

    let generation = clientGeneration
    let operationClient = client
    creatingPerformanceSampleID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        creatingPerformanceSampleID = nil
      }
    }

    do {
      let sample = try await operationClient.createPerformanceSample(
        output: output,
        artifactBytes: artifactBytes,
        iterations: iterations,
        sync: sync,
        on: virtualMachine.id
      )
      guard generation == clientGeneration else {
        return false
      }
      performanceSamples[virtualMachine.id] = sample
      performanceSampleErrors[virtualMachine.id] = nil
      alertMessage = "Performance sample metadata created at \(sample.artifact)."
      return true
    } catch {
      guard generation == clientGeneration else {
        return false
      }
      let message = error.localizedDescription
      performanceSampleErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func loadLogView(kind: VMLogKind, for virtualMachine: VirtualMachine) async {
    guard loadingLogViewID != virtualMachine.id else {
      return
    }

    let generation = clientGeneration
    let operationClient = client
    loadingLogViewID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        loadingLogViewID = nil
      }
    }

    do {
      let log = try await operationClient.viewLogs(
        kind: kind, bytes: 16 * 1024, on: virtualMachine.id)
      guard generation == clientGeneration else {
        return
      }
      var logs = logViews[virtualMachine.id] ?? [:]
      logs[kind] = log
      logViews[virtualMachine.id] = logs
      logViewErrors[virtualMachine.id] = nil
    } catch {
      guard generation == clientGeneration else {
        return
      }
      let message = error.localizedDescription
      logViewErrors[virtualMachine.id] = message
      alertMessage = message
    }
  }

  /// Open a Fast Mode (Apple VZ) VM in an embedded display window by spawning
  /// the bundled runner with `--apple-vz-display`. Local-GUI only and outside the
  /// daemon path (the window must live on the user's session).
  func showDisplay(for virtualMachine: VirtualMachine) {
    guard virtualMachine.mode == .fast else {
      alertMessage = "The embedded display window is available for Fast Mode VMs only."
      return
    }
    do {
      try EmbeddedDisplayLauncher.launch(vmName: virtualMachine.name)
      alertMessage =
        "Opening an embedded display window for \(virtualMachine.name) (close the window to stop the VM)."
    } catch {
      alertMessage = error.localizedDescription
    }
  }

  func openConsole(for virtualMachine: VirtualMachine) async -> Bool {
    let capability = ConsoleCapability.evaluate(for: virtualMachine)
    guard capability.qmpDiagnosticsAvailable else {
      alertMessage =
        "Console diagnostics and VNC viewer handoff are available for running or paused virtual machines."
      return false
    }

    let generation = clientGeneration
    let operationClient = client
    guard openingConsoleID != virtualMachine.id else {
      return false
    }

    openingConsoleID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        openingConsoleID = nil
      }
    }

    do {
      if let viewerEndpoint = try await consoleViewerEndpoint(
        for: virtualMachine, generation: generation, operationClient: operationClient)
      {
        guard generation == clientGeneration else {
          return false
        }
        if openExternalURL(viewerEndpoint) {
          qemuLaunchPlanErrors[virtualMachine.id] = nil
          alertMessage = "Opened VNC viewer at \(viewerEndpoint.absoluteString)."
          return true
        }

        let message =
          "macOS could not open \(viewerEndpoint.absoluteString). Open it manually with your VNC viewer."
        qemuLaunchPlanErrors[virtualMachine.id] = message
        return await probeQMPDiagnostics(
          for: virtualMachine,
          generation: generation,
          operationClient: operationClient,
          fallback: .viewerHandoffFailed(message)
        )
      }
    } catch {
      guard generation == clientGeneration else {
        return false
      }
      let message = error.localizedDescription
      qemuLaunchPlanErrors[virtualMachine.id] = message
      return await probeQMPDiagnostics(
        for: virtualMachine,
        generation: generation,
        operationClient: operationClient,
        fallback: .viewerEndpointUnavailable(message)
      )
    }

    guard generation == clientGeneration else {
      return false
    }

    return await probeQMPDiagnostics(
      for: virtualMachine,
      generation: generation,
      operationClient: operationClient
    )
  }

  private func probeQMPDiagnostics(
    for virtualMachine: VirtualMachine,
    generation: Int,
    operationClient: VirtualMachineClient,
    fallback: ConsoleFallback? = nil
  ) async -> Bool {
    do {
      let qmp = try await operationClient.inspectQMPStatus(on: virtualMachine.id)
      guard generation == clientGeneration else {
        return false
      }
      qmpStatuses[virtualMachine.id] = qmp
      qmpStatusErrors[virtualMachine.id] = nil
      alertMessage = consoleMessage(for: qmp, fallback: fallback)
      return qmp.available
    } catch {
      guard generation == clientGeneration else {
        return false
      }
      let message = error.localizedDescription
      qmpStatuses[virtualMachine.id] = nil
      qmpStatusErrors[virtualMachine.id] = message
      if let fallback {
        alertMessage =
          "\(fallback.namedMessage(for: virtualMachine)). QMP diagnostics also failed: \(message)"
      } else {
        alertMessage = message
      }
      return false
    }
  }

  private func consoleViewerEndpoint(
    for virtualMachine: VirtualMachine,
    generation: Int,
    operationClient: VirtualMachineClient
  ) async throws -> URL? {
    guard virtualMachine.mode == .compatibility else {
      return nil
    }

    if let viewerEndpoint = qemuLaunchPlans[virtualMachine.id]?.viewerEndpoint {
      return viewerEndpoint
    }

    let plan = try await operationClient.inspectQemuArgs(on: virtualMachine.id)
    guard generation == clientGeneration else {
      return nil
    }
    qemuLaunchPlans[virtualMachine.id] = plan
    qemuLaunchPlanErrors[virtualMachine.id] = nil
    return plan.viewerEndpoint
  }

  func importBootMedia(
    sourcePath: String,
    kind: BootMediaStatusEntry.Kind?,
    for virtualMachine: VirtualMachine
  ) async -> Bool {
    guard canMutateCurrentInventory(action: "Import boot media", virtualMachine: virtualMachine)
    else {
      return false
    }

    let trimmedSourcePath = sourcePath.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedSourcePath.isEmpty else {
      alertMessage = "Enter a local source path to import."
      return false
    }

    guard importingBootMediaID != virtualMachine.id else {
      return false
    }

    importingBootMediaID = virtualMachine.id
    defer { importingBootMediaID = nil }

    do {
      let metadata = try await client.importBootMedia(
        sourcePath: trimmedSourcePath,
        kind: kind?.isImportable == true ? kind : nil,
        on: virtualMachine.id
      )
      clearReadinessCaches(for: virtualMachine.id)
      alertMessage = "\(metadata.kind.title) imported from \(metadata.source)."
      await loadBootMediaStatus(for: virtualMachine)
      return true
    } catch {
      let message = error.localizedDescription
      bootMediaStatusErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func verifyBootMedia(
    expectedSHA256: String,
    kind: BootMediaStatusEntry.Kind?,
    for virtualMachine: VirtualMachine
  ) async -> Bool {
    guard canMutateCurrentInventory(action: "Verify boot media", virtualMachine: virtualMachine)
    else {
      return false
    }

    let trimmedExpectedSHA256 = expectedSHA256.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedExpectedSHA256.isEmpty else {
      alertMessage = "Enter the expected SHA256 hash."
      return false
    }

    guard verifyingBootMediaID != virtualMachine.id else {
      return false
    }

    verifyingBootMediaID = virtualMachine.id
    defer { verifyingBootMediaID = nil }

    do {
      let metadata = try await client.verifyBootMedia(
        expectedSHA256: trimmedExpectedSHA256,
        kind: kind?.isImportable == true ? kind : nil,
        on: virtualMachine.id
      )
      clearReadinessCaches(for: virtualMachine.id)
      alertMessage =
        metadata.verified
        ? "\(metadata.kind.title) verified."
        : "\(metadata.kind.title) verification failed."
      await loadBootMediaStatus(for: virtualMachine)
      return metadata.verified
    } catch {
      let message = error.localizedDescription
      bootMediaStatusErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func planBootMediaDownload(
    url: String,
    expectedSHA256: String?,
    kind: BootMediaStatusEntry.Kind?,
    for virtualMachine: VirtualMachine
  ) async -> Bool {
    guard canMutateCurrentInventory(
      action: "Plan boot media download",
      virtualMachine: virtualMachine
    ) else {
      return false
    }

    let trimmedURL = url.trimmingCharacters(in: .whitespacesAndNewlines)
    let trimmedExpectedSHA256 = expectedSHA256?.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedURL.isEmpty else {
      alertMessage = "Enter an HTTPS URL to plan."
      return false
    }
    guard URL(string: trimmedURL)?.scheme?.lowercased() == "https" else {
      alertMessage = "Boot media download plans require an HTTPS URL."
      return false
    }

    guard planningBootMediaDownloadID != virtualMachine.id else {
      return false
    }

    planningBootMediaDownloadID = virtualMachine.id
    defer { planningBootMediaDownloadID = nil }

    do {
      let plan = try await client.planBootMediaDownload(
        url: trimmedURL,
        expectedSHA256: trimmedExpectedSHA256?.isEmpty == false ? trimmedExpectedSHA256 : nil,
        kind: kind?.isImportable == true ? kind : nil,
        on: virtualMachine.id
      )
      clearReadinessCaches(for: virtualMachine.id)
      alertMessage = "\(plan.kind.title) download planned for \(plan.destination)."
      await loadBootMediaStatus(for: virtualMachine)
      return true
    } catch {
      let message = error.localizedDescription
      bootMediaStatusErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func downloadBootMedia(
    kind: BootMediaStatusEntry.Kind?,
    for virtualMachine: VirtualMachine
  ) async -> Bool {
    guard canMutateCurrentInventory(action: "Download boot media", virtualMachine: virtualMachine)
    else {
      return false
    }

    guard downloadingBootMediaID != virtualMachine.id else {
      return false
    }

    downloadingBootMediaID = virtualMachine.id
    defer { downloadingBootMediaID = nil }

    do {
      let download = try await client.downloadBootMedia(
        kind: kind?.isImportable == true ? kind : nil,
        on: virtualMachine.id
      )
      clearReadinessCaches(for: virtualMachine.id)
      alertMessage =
        download.downloaded
        ? "\(download.kind.title) downloaded to \(download.destination)."
        : "\(download.kind.title) download failed."
      await loadBootMediaStatus(for: virtualMachine)
      return download.downloaded
    } catch {
      let message = error.localizedDescription
      bootMediaStatusErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func createVirtualMachine(name: String, templateID: BootTemplate.ID?) async -> Bool {
    let trimmedName = name.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedName.isEmpty else {
      alertMessage = "Enter a virtual machine name."
      return false
    }

    guard let template = bootTemplates.first(where: { $0.id == templateID }) ?? bootTemplates.first
    else {
      alertMessage = "No boot templates are available."
      return false
    }

    isCreatingVirtualMachine = true
    defer { isCreatingVirtualMachine = false }

    do {
      let created = try await client.createVirtualMachine(
        CreateVirtualMachineRequest(name: trimmedName, template: template)
      )
      await load()
      selection = virtualMachines.first(where: { $0.name == created.name })?.id ?? created.id
      alertMessage = "\(created.name) created."
      return true
    } catch {
      alertMessage = error.localizedDescription
      return false
    }
  }

  func cloneVirtualMachine(name: String, linked: Bool, for virtualMachine: VirtualMachine) async
    -> Bool
  {
    guard canMutateCurrentInventory(action: "Clone VM", virtualMachine: virtualMachine) else {
      return false
    }

    let trimmedName = name.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmedName.isEmpty else {
      alertMessage = "Enter a clone name."
      return false
    }

    guard cloningVirtualMachineID != virtualMachine.id else {
      return false
    }

    cloningVirtualMachineID = virtualMachine.id
    defer { cloningVirtualMachineID = nil }

    do {
      let metadata = try await client.cloneVirtualMachine(
        on: virtualMachine.id,
        newName: trimmedName,
        linked: linked
      )
      await load()
      selection = virtualMachines.first(where: { $0.name == metadata.vm })?.id
        ?? virtualMachine.id
      alertMessage =
        metadata.linked
        ? "\(metadata.vm) linked clone created from \(virtualMachine.name)."
        : "\(metadata.vm) cloned from \(virtualMachine.name)."
      return true
    } catch {
      alertMessage = error.localizedDescription
      return false
    }
  }

  func deleteVirtualMachine(_ virtualMachine: VirtualMachine) async -> Bool {
    guard canMutateCurrentInventory(action: "Delete VM", virtualMachine: virtualMachine) else {
      return false
    }

    guard virtualMachine.status == .stopped else {
      alertMessage = "Stop \(virtualMachine.name) before deleting it."
      return false
    }

    guard deletingVirtualMachineID != virtualMachine.id else {
      return false
    }

    deletingVirtualMachineID = virtualMachine.id
    defer { deletingVirtualMachineID = nil }

    do {
      let metadata = try await client.deleteVirtualMachine(on: virtualMachine.id)
      guard metadata.metadataOnly else {
        alertMessage = "Delete response for \(virtualMachine.name) was not metadata-only."
        return false
      }

      await load()
      selection = nil
      alertMessage = "\(metadata.vm ?? virtualMachine.name) deleted from metadata at \(metadata.path)."
      return true
    } catch {
      alertMessage = error.localizedDescription
      return false
    }
  }

  func exportVirtualMachine(output outputText: String, for virtualMachine: VirtualMachine) async
    -> Bool
  {
    guard canMutateCurrentInventory(action: "Export VM", virtualMachine: virtualMachine) else {
      return false
    }

    let output = outputText.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !output.isEmpty else {
      alertMessage = "Enter an export output path."
      return false
    }

    guard exportingVirtualMachineID != virtualMachine.id else {
      return false
    }

    exportingVirtualMachineID = virtualMachine.id
    defer { exportingVirtualMachineID = nil }

    do {
      let metadata = try await client.exportVirtualMachine(on: virtualMachine.id, output: output)
      vmExports[virtualMachine.id] = metadata
      vmExportErrors[virtualMachine.id] = nil
      alertMessage = "\(metadata.vm) exported to \(metadata.output)."
      return true
    } catch {
      let message = error.localizedDescription
      vmExportErrors[virtualMachine.id] = message
      alertMessage = message
      return false
    }
  }

  func importVirtualMachine(input inputText: String, name nameText: String) async -> Bool {
    let input = inputText.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !input.isEmpty else {
      alertMessage = "Enter an import input path."
      return false
    }

    guard !isImportingVirtualMachine else {
      return false
    }

    isImportingVirtualMachine = true
    defer { isImportingVirtualMachine = false }

    do {
      let metadata = try await client.importVirtualMachine(
        input: input,
        name: optionalTrimmed(nameText)
      )
      lastVMImport = metadata
      vmImportError = nil
      await load()
      selection = virtualMachines.first(where: { $0.name == metadata.vm })?.id ?? selection
      alertMessage = "\(metadata.vm) imported from \(metadata.source)."
      return true
    } catch {
      let message = error.localizedDescription
      vmImportError = message
      alertMessage = message
      return false
    }
  }

  func performPrimaryAction(on virtualMachine: VirtualMachine) async {
    switch virtualMachine.status {
    case .running:
      await perform(.pause, on: virtualMachine)
    case .paused, .suspended:
      await perform(.resume, on: virtualMachine)
    case .stopped, .error:
      await perform(.start, on: virtualMachine)
    }
  }

  private func readinessSummary(for virtualMachine: VirtualMachine) -> VMReadinessSummary {
    VMReadinessSummary.evaluate(
      virtualMachine: virtualMachine,
      bootMediaStatus: bootMediaStatuses[virtualMachine.id],
      bootMediaStatusError: bootMediaStatusErrors[virtualMachine.id],
      runnerStatus: runnerStatuses[virtualMachine.id],
      runnerStatusError: runnerStatusErrors[virtualMachine.id],
      preRunLaunchReadiness: runnerStatuses[virtualMachine.id] == nil
        ? readinessReports[virtualMachine.id]?.preRunLaunchReadiness
        : nil,
      snapshotChain: snapshotChains[virtualMachine.id],
      snapshotChainError: snapshotChainErrors[virtualMachine.id],
      diskPreparation: diskPreparations[virtualMachine.id],
      diskCreation: diskCreations[virtualMachine.id],
      diskInspection: diskInspections[virtualMachine.id],
      diskVerification: diskVerifications[virtualMachine.id]
    )
  }

  private func canStart(_ virtualMachine: VirtualMachine) async -> Bool {
    if runnerStatuses[virtualMachine.id]?.launchReadiness?.ready == true {
      return true
    }

    if runnerStatuses[virtualMachine.id] != nil {
      _ = await prepareRun(for: virtualMachine)
      return runnerStatuses[virtualMachine.id]?.launchReadiness?.ready == true
    }

    if readinessReports[virtualMachine.id] == nil {
      await loadReadinessReport(for: virtualMachine)
    }

    if runnerStatuses[virtualMachine.id]?.launchReadiness?.ready == true {
      return true
    }

    if runnerStatuses[virtualMachine.id] != nil {
      _ = await prepareRun(for: virtualMachine)
      return runnerStatuses[virtualMachine.id]?.launchReadiness?.ready == true
    }

    let summary = readinessSummary(for: virtualMachine)

    guard summary.action == .primaryAction, summary.severity == .ready else {
      if summary.action == .prepareRun {
        let prepared = await prepareRun(for: virtualMachine)
        if prepared, runnerStatuses[virtualMachine.id]?.launchReadiness?.ready == true {
          return true
        }
      }
      alertMessage = "\(summary.title): \(summary.detail)"
      return false
    }

    return true
  }

  private func canRestart(_ virtualMachine: VirtualMachine) async -> Bool {
    let prepared = await prepareRun(for: virtualMachine)
    guard prepared else {
      return false
    }

    guard let readiness = runnerStatuses[virtualMachine.id]?.launchReadiness else {
      alertMessage = "\(virtualMachine.name) restart blocked: launch readiness was not reported."
      return false
    }

    guard readiness.ready else {
      let blockers = readiness.blockers.map(\.code).joined(separator: ", ")
      let detail = blockers.isEmpty ? "launch readiness is blocked" : blockers
      alertMessage = "\(virtualMachine.name) restart blocked: \(detail)."
      return false
    }

    return true
  }

  func lifecycleActions(for virtualMachine: VirtualMachine) -> [LifecycleActionOption] {
    switch virtualMachine.status {
    case .running:
      return [
        LifecycleActionOption(
          action: .pause,
          title: "Suspend",
          detail: "Pause this VM and save its machine state to disk.",
          systemImage: "pause.fill"
        ),
        LifecycleActionOption(
          action: .restart,
          title: "Restart",
          detail: "Stop metadata-backed runtime state, then mark it running again.",
          systemImage: "arrow.clockwise"
        ),
        LifecycleActionOption(
          action: .stop,
          title: "Stop",
          detail: "Clear runtime state without deleting the VM bundle.",
          systemImage: "stop.fill",
          isDestructive: true
        ),
      ]
    case .paused, .suspended:
      return [
        LifecycleActionOption(
          action: .resume,
          title: "Resume",
          detail: "Restore this VM from its saved machine state and run it.",
          systemImage: "play.fill"
        ),
        LifecycleActionOption(
          action: .stop,
          title: "Stop",
          detail: "Clear suspended runtime state without deleting the VM bundle.",
          systemImage: "stop.fill",
          isDestructive: true
        ),
      ]
    case .stopped:
      return [
        LifecycleActionOption(
          action: .start,
          title: "Start",
          detail: "Prepare launch readiness, then ask the daemon to launch the backend when ready.",
          systemImage: "play.fill"
        )
      ]
    case .error:
      return [
        LifecycleActionOption(
          action: .start,
          title: "Start",
          detail: "Retry launch readiness, then ask the daemon to launch the backend when ready.",
          systemImage: "play.fill"
        ),
        LifecycleActionOption(
          action: .stop,
          title: "Stop",
          detail: "Clear runtime state after a failed launch attempt.",
          systemImage: "stop.fill",
          isDestructive: true
        ),
      ]
    }
  }

  func perform(_ action: VirtualMachineAction, on virtualMachine: VirtualMachine) async {
    guard activeActionID == nil else {
      return
    }

    guard canMutateCurrentInventory(action: action.pastTenseMessage.capitalized, virtualMachine: virtualMachine)
    else {
      return
    }

    if action == .start, !(await canStart(virtualMachine)) {
      return
    }

    if action == .restart, !(await canRestart(virtualMachine)) {
      return
    }

    let generation = clientGeneration
    let operationClient = client
    activeActionID = virtualMachine.id
    defer {
      if generation == clientGeneration {
        activeActionID = nil
      }
    }

    do {
      let result = try await operationClient.perform(action, on: virtualMachine.id)
      guard generation == clientGeneration else {
        return
      }
      replace(result.virtualMachine)
      selection = result.virtualMachine.id
      clearReadinessCaches(for: result.virtualMachine.id)
      if action == .start || action == .restart {
        await refreshPostLaunchCaches(for: result.virtualMachine, generation: generation)
      } else if action == .stop {
        clearRuntimeCaches(for: result.virtualMachine.id)
      }
      alertMessage = result.message
    } catch {
      guard generation == clientGeneration else {
        return
      }
      alertMessage = error.localizedDescription
    }
  }

  private func refreshPostLaunchCaches(for virtualMachine: VirtualMachine, generation: Int) async {
    let id = virtualMachine.id
    let operationClient = client
    async let runnerResult = inspectRunnerStatusResult(on: id, client: operationClient)
    async let qemuLaunchPlanResult = inspectQemuLaunchPlanResult(on: id, client: operationClient)
    async let guestToolsStatusResult = inspectGuestToolsStatusResult(on: id, client: operationClient)

    switch await runnerResult {
    case .success(let status):
      guard generation == clientGeneration else {
        return
      }
      runnerStatuses[id] = status
      runnerStatusErrors[id] = nil
    case .failure(let error):
      guard generation == clientGeneration else {
        return
      }
      runnerStatusErrors[id] = error.localizedDescription
    }

    switch await qemuLaunchPlanResult {
    case .success(let plan):
      guard generation == clientGeneration else {
        return
      }
      qemuLaunchPlans[id] = plan
      qemuLaunchPlanErrors[id] = nil
    case .failure(let error):
      guard generation == clientGeneration else {
        return
      }
      qemuLaunchPlanErrors[id] = error.localizedDescription
    }

    switch await guestToolsStatusResult {
    case .success(let status):
      guard generation == clientGeneration else {
        return
      }
      guestToolsStatuses[id] = status
      guestToolsStatusErrors[id] = nil
    case .failure(let error):
      guard generation == clientGeneration else {
        return
      }
      guestToolsStatusErrors[id] = error.localizedDescription
    }
  }

  private func inspectRunnerStatusResult(
    on id: VirtualMachine.ID,
    client: VirtualMachineClient
  ) async -> Result<
    RunnerStatus?, Error
  > {
    do {
      return .success(try await client.inspectRunnerStatus(on: id))
    } catch {
      return .failure(error)
    }
  }

  private func inspectQemuLaunchPlanResult(
    on id: VirtualMachine.ID,
    client: VirtualMachineClient
  ) async -> Result<
    QemuLaunchPlan, Error
  > {
    do {
      return .success(try await client.inspectQemuArgs(on: id))
    } catch {
      return .failure(error)
    }
  }

  private func inspectGuestToolsStatusResult(
    on id: VirtualMachine.ID,
    client: VirtualMachineClient
  ) async -> Result<
    GuestToolsStatus, Error
  > {
    do {
      return .success(try await client.inspectGuestToolsStatus(on: id))
    } catch {
      return .failure(error)
    }
  }

  private func clearRuntimeCaches(for id: VirtualMachine.ID) {
    runnerStatuses[id] = nil
    runnerStatusErrors[id] = nil
    qmpStatuses[id] = nil
    qmpStatusErrors[id] = nil
    qemuLaunchPlans[id] = nil
    qemuLaunchPlanErrors[id] = nil
    guestToolsStatuses[id] = nil
    guestToolsStatusErrors[id] = nil
  }

  private func clearReadinessCaches(for id: VirtualMachine.ID) {
    readinessReports[id] = nil
    readinessReportErrors[id] = nil
    bootMediaStatuses[id] = nil
    bootMediaStatusErrors[id] = nil
    snapshotChains[id] = nil
    snapshotChainErrors[id] = nil
    runnerStatuses[id] = nil
    runnerStatusErrors[id] = nil
    qemuLaunchPlans[id] = nil
    qemuLaunchPlanErrors[id] = nil
  }

  private func invalidateReadinessCaches(
    changingFrom oldVirtualMachines: [VirtualMachine],
    to newVirtualMachines: [VirtualMachine]
  ) {
    let newByID = Dictionary(uniqueKeysWithValues: newVirtualMachines.map { ($0.id, $0) })
    for oldVirtualMachine in oldVirtualMachines {
      guard newByID[oldVirtualMachine.id] == oldVirtualMachine else {
        clearReadinessCaches(for: oldVirtualMachine.id)
        continue
      }
    }
  }

  private func replace(_ virtualMachine: VirtualMachine) {
    guard let index = virtualMachines.firstIndex(where: { $0.id == virtualMachine.id }) else {
      return
    }

    virtualMachines[index] = virtualMachine
  }

  private func beginSnapshotMetadataMutation(for virtualMachine: VirtualMachine) -> Bool {
    guard !snapshotMetadataMutationIDs.contains(virtualMachine.id) else {
      return false
    }

    snapshotMetadataMutationIDs.insert(virtualMachine.id)
    return true
  }

  private func endSnapshotMetadataMutation(for virtualMachine: VirtualMachine) {
    snapshotMetadataMutationIDs.remove(virtualMachine.id)
  }

  private enum ConsoleFallback {
    case viewerHandoffFailed(String)
    case viewerEndpointUnavailable(String)

    private var title: String {
      switch self {
      case .viewerHandoffFailed:
        return "VNC viewer handoff failed"
      case .viewerEndpointUnavailable:
        return "VNC viewer endpoint unavailable"
      }
    }

    private var reason: String {
      let raw: String
      switch self {
      case .viewerHandoffFailed(let message), .viewerEndpointUnavailable(let message):
        raw = message
      }
      return raw.trimmingCharacters(in: CharacterSet(charactersIn: ". "))
    }

    var message: String {
      "\(title): \(reason)"
    }

    func namedMessage(for virtualMachine: VirtualMachine) -> String {
      "\(title) for \(virtualMachine.name): \(reason)"
    }
  }

  private func consoleMessage(for qmp: QMPStatus, fallback: ConsoleFallback? = nil) -> String {
    guard qmp.available else {
      let message = "Console diagnostics are not available yet. Expected QMP socket: \(qmp.socketPath)"
      guard let fallback else {
        return message
      }
      return "\(fallback.message). \(message)"
    }

    let prefix = fallback.map { "\($0.message). " } ?? ""
    if let supervisor = qmp.supervisor {
      return "\(prefix)QMP diagnostics socket available at \(qmp.socketPath) (\(qmp.readinessTitle)); supervisor cache: \(supervisor.summaryTitle)."
    }

    return "\(prefix)QMP diagnostics socket available at \(qmp.socketPath) (\(qmp.readinessTitle))."
  }

  private func makeGuestToolsRequestID(for command: GuestToolsAgentCommand) -> String {
    let prefix: String
    switch command {
    case .listApplications:
      prefix = "apps"
    case .listWindows:
      prefix = "windows"
    case .setClipboard:
      prefix = "clipboard"
    case .resizeDisplay:
      prefix = "display"
    case .unmountShare:
      prefix = "unmount-share"
    case .fileDropStart, .fileDropChunk, .fileDropComplete:
      prefix = "file-drop"
    case .launchApplication:
      prefix = "launch-app"
    case .focusWindow:
      prefix = "focus-window"
    case .closeWindow:
      prefix = "close-window"
    case .timeSync:
      prefix = "time-sync"
    }

    let millis = UInt64(Date().timeIntervalSince1970 * 1_000)
    return "\(prefix)-\(millis)"
  }

  private func optionalTrimmed(_ value: String) -> String? {
    let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? nil : trimmed
  }

  private func requiredMetadataOutput(_ value: String) -> String? {
    optionalTrimmed(value)
  }
}
