import Foundation
import Network

enum VirtualMachineAction: Equatable {
  case start
  case pause
  case resume
  case stop
  case restart

  var pastTenseMessage: String {
    switch self {
    case .start:
      return "started"
    case .pause:
      return "suspended"
    case .resume:
      return "resumed"
    case .stop:
      return "stopped"
    case .restart:
      return "restarted"
    }
  }
}

protocol VirtualMachineClient: StoreDoctorInspecting {
  func listVirtualMachines() async throws -> [VirtualMachine]
  func listBootTemplates() async throws -> [BootTemplate]
  func inspectReadinessReport(on id: VirtualMachine.ID) async throws -> VMReadinessReport
  func inspectBootMediaStatus(on id: VirtualMachine.ID) async throws -> BootMediaStatus
  func importBootMedia(
    sourcePath: String,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaImportMetadata
  func verifyBootMedia(
    expectedSHA256: String,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaVerificationMetadata
  func planBootMediaDownload(
    url: String,
    expectedSHA256: String?,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaDownloadPlanMetadata
  func downloadBootMedia(
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaDownloadResultMetadata
  func inspectLifecyclePlan(action: LifecyclePlanAction, on id: VirtualMachine.ID) async throws
    -> LifecyclePlan
  func inspectOpenPortPlan(
    guestPort: UInt16,
    scheme: String,
    on id: VirtualMachine.ID
  ) async throws -> OpenPortPlan
  func inspectNetworkPlan(on id: VirtualMachine.ID) async throws -> NetworkPlan
  func inspectSSHPlan(user: String, on id: VirtualMachine.ID) async throws -> SSHPlan
  func listPortForwards(on id: VirtualMachine.ID) async throws -> VMPortForwardList
  func addPortForward(host: UInt16, guest: UInt16, on id: VirtualMachine.ID) async throws
    -> VMPortForwardList
  func removePortForward(host: UInt16, guest: UInt16, on id: VirtualMachine.ID) async throws
    -> VMPortForwardList
  func listSharedFolders(on id: VirtualMachine.ID) async throws -> VMSharedFolderList
  func addSharedFolder(
    named shareName: String,
    hostPath: String,
    readOnly: Bool,
    hostPathToken: String?,
    on id: VirtualMachine.ID
  ) async throws -> VMSharedFolderList
  func removeSharedFolder(named shareName: String, on id: VirtualMachine.ID) async throws
    -> VMSharedFolderList
  func inspectGuestToolsStatus(on id: VirtualMachine.ID) async throws -> GuestToolsStatus
  func inspectGuestToolsToken(on id: VirtualMachine.ID) async throws -> GuestToolsToken
  func inspectGuestToolsLinuxCommand(
    transport: GuestToolsLinuxCommandTransport,
    on id: VirtualMachine.ID
  ) async throws -> GuestToolsLinuxCommand
  func mountApprovedSharedFolder(named shareName: String, on id: VirtualMachine.ID) async throws
    -> GuestToolsStatus?
  func unmountApprovedSharedFolder(named shareName: String, on id: VirtualMachine.ID) async throws
    -> GuestToolsStatus?
  func sendGuestToolsCommand(
    _ command: GuestToolsAgentCommand,
    requestID: String?,
    on id: VirtualMachine.ID
  ) async throws -> GuestToolsCommandDispatch
  func inspectSnapshotPreflightStatus(on id: VirtualMachine.ID) async throws
    -> SnapshotPreflightStatus
  func listSnapshots(on id: VirtualMachine.ID) async throws -> [VMSnapshot]
  func inspectSnapshotChain(on id: VirtualMachine.ID) async throws -> VMSnapshotChain
  func createSnapshot(named snapshotName: String, kind: VMSnapshotKind, on id: VirtualMachine.ID)
    async throws -> VMSnapshot
  func createSnapshotDisk(named snapshotName: String, on id: VirtualMachine.ID) async throws
    -> VMSnapshotDiskCreation
  func preparePrimaryDisk(on id: VirtualMachine.ID) async throws -> DiskPreparation
  func createPrimaryDisk(on id: VirtualMachine.ID) async throws -> VMDiskCreation
  func inspectPrimaryDisk(on id: VirtualMachine.ID) async throws -> VMDiskInspection
  func verifyActiveDisk(on id: VirtualMachine.ID) async throws -> VMDiskVerification
  func compactActiveDisk(on id: VirtualMachine.ID) async throws -> VMDiskCompaction
  func repairMetadata(on id: VirtualMachine.ID) async throws -> VMMetadataRepair
  func migrateManifest(on id: VirtualMachine.ID, dryRun: Bool) async throws -> VMManifestMigration
  func restoreSnapshot(named snapshotName: String, on id: VirtualMachine.ID) async throws
    -> SnapshotRestoreResult
  func executeApplicationConsistentSnapshot(
    named snapshotName: String,
    freezeTimeoutMillis: UInt64?,
    on id: VirtualMachine.ID
  ) async throws -> ApplicationConsistentSnapshotExecution
  func reapplyRuntimeResources(
    visibility: RuntimeResourceVisibility,
    on id: VirtualMachine.ID
  ) async throws -> RuntimeResourcePolicy
  func createDiagnosticBundle(output: String?, on id: VirtualMachine.ID) async throws
    -> DiagnosticBundle
  func createPerformanceBaseline(output: String?, on id: VirtualMachine.ID) async throws
    -> PerformanceBaseline
  func createPerformanceSample(
    output: String?,
    artifactBytes: UInt64,
    iterations: UInt16,
    sync: Bool,
    on id: VirtualMachine.ID
  ) async throws -> PerformanceSample
  func inspectQMPStatus(on id: VirtualMachine.ID) async throws -> QMPStatus
  func viewLogs(kind: VMLogKind, bytes: UInt64?, on id: VirtualMachine.ID) async throws
    -> VMLogView
  func inspectQemuArgs(on id: VirtualMachine.ID) async throws -> QemuLaunchPlan
  func prepareRun(on id: VirtualMachine.ID) async throws -> RunnerStatus
  func inspectRunnerStatus(on id: VirtualMachine.ID) async throws -> RunnerStatus?
  func recommendMode(for choice: GuestChoice) async throws -> ModeRecommendation
  func exportVirtualMachine(on id: VirtualMachine.ID, output: String) async throws
    -> VMExportMetadata
  func importVirtualMachine(input: String, name: String?) async throws -> VMImportMetadata
  func createVirtualMachine(_ request: CreateVirtualMachineRequest) async throws -> VirtualMachine
  func cloneVirtualMachine(on id: VirtualMachine.ID, newName: String, linked: Bool) async throws
    -> CloneVirtualMachineMetadata
  func deleteVirtualMachine(on id: VirtualMachine.ID) async throws -> VMDeletionMetadata
  func perform(_ action: VirtualMachineAction, on id: VirtualMachine.ID) async throws
    -> VMActionResult
}

extension VirtualMachineClient {
  func inspectStoreDoctor() async throws -> StoreDoctorReport {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func inspectLifecyclePlan(action: LifecyclePlanAction, on id: VirtualMachine.ID) async throws
    -> LifecyclePlan
  {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func inspectReadinessReport(on id: VirtualMachine.ID) async throws -> VMReadinessReport {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func reapplyRuntimeResources(
    visibility: RuntimeResourceVisibility,
    on id: VirtualMachine.ID
  ) async throws -> RuntimeResourcePolicy {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func deleteVirtualMachine(on id: VirtualMachine.ID) async throws -> VMDeletionMetadata {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func inspectOpenPortPlan(
    guestPort: UInt16,
    scheme: String,
    on id: VirtualMachine.ID
  ) async throws -> OpenPortPlan {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func inspectNetworkPlan(on id: VirtualMachine.ID) async throws -> NetworkPlan {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func inspectSSHPlan(user: String, on id: VirtualMachine.ID) async throws -> SSHPlan {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func listPortForwards(on id: VirtualMachine.ID) async throws -> VMPortForwardList {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func addPortForward(host: UInt16, guest: UInt16, on id: VirtualMachine.ID) async throws
    -> VMPortForwardList
  {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func removePortForward(host: UInt16, guest: UInt16, on id: VirtualMachine.ID) async throws
    -> VMPortForwardList
  {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func listSharedFolders(on id: VirtualMachine.ID) async throws -> VMSharedFolderList {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func addSharedFolder(
    named shareName: String,
    hostPath: String,
    readOnly: Bool,
    hostPathToken: String?,
    on id: VirtualMachine.ID
  ) async throws -> VMSharedFolderList {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func removeSharedFolder(named shareName: String, on id: VirtualMachine.ID) async throws
    -> VMSharedFolderList
  {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func inspectGuestToolsToken(on id: VirtualMachine.ID) async throws -> GuestToolsToken {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func inspectGuestToolsLinuxCommand(
    transport: GuestToolsLinuxCommandTransport,
    on id: VirtualMachine.ID
  ) async throws -> GuestToolsLinuxCommand {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func mountApprovedSharedFolder(named shareName: String, on id: VirtualMachine.ID) async throws
    -> GuestToolsStatus?
  {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func unmountApprovedSharedFolder(named shareName: String, on id: VirtualMachine.ID) async throws
    -> GuestToolsStatus?
  {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func prepareRun(on id: VirtualMachine.ID) async throws -> RunnerStatus {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func inspectQemuArgs(on id: VirtualMachine.ID) async throws -> QemuLaunchPlan {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func listSnapshots(on id: VirtualMachine.ID) async throws -> [VMSnapshot] {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func restoreSnapshot(named snapshotName: String, on id: VirtualMachine.ID) async throws
    -> SnapshotRestoreResult
  {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func inspectSnapshotChain(on id: VirtualMachine.ID) async throws -> VMSnapshotChain {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func createSnapshot(named snapshotName: String, kind: VMSnapshotKind, on id: VirtualMachine.ID)
    async throws -> VMSnapshot
  {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func createSnapshotDisk(named snapshotName: String, on id: VirtualMachine.ID) async throws
    -> VMSnapshotDiskCreation
  {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func preparePrimaryDisk(on id: VirtualMachine.ID) async throws -> DiskPreparation {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func createPrimaryDisk(on id: VirtualMachine.ID) async throws -> VMDiskCreation {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func inspectPrimaryDisk(on id: VirtualMachine.ID) async throws -> VMDiskInspection {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func verifyActiveDisk(on id: VirtualMachine.ID) async throws -> VMDiskVerification {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func compactActiveDisk(on id: VirtualMachine.ID) async throws -> VMDiskCompaction {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func repairMetadata(on id: VirtualMachine.ID) async throws -> VMMetadataRepair {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func migrateManifest(on id: VirtualMachine.ID, dryRun: Bool) async throws -> VMManifestMigration {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func createDiagnosticBundle(output: String?, on id: VirtualMachine.ID) async throws
    -> DiagnosticBundle
  {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func createPerformanceBaseline(output: String?, on id: VirtualMachine.ID) async throws
    -> PerformanceBaseline
  {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func createPerformanceSample(
    output: String?,
    artifactBytes: UInt64,
    iterations: UInt16,
    sync: Bool,
    on id: VirtualMachine.ID
  ) async throws -> PerformanceSample {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func exportVirtualMachine(on id: VirtualMachine.ID, output: String) async throws
    -> VMExportMetadata
  {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func importVirtualMachine(input: String, name: String?) async throws -> VMImportMetadata {
    throw VirtualMachineClientError.daemonResponseInvalid
  }

  func recommendMode(for choice: GuestChoice) async throws -> ModeRecommendation {
    throw VirtualMachineClientError.daemonResponseInvalid
  }
}

protocol VirtualMachineClientSourceProviding {
  var sourceTitle: String { get }
  var allowsMutationsForCurrentInventory: Bool { get }
}

extension VirtualMachineClientSourceProviding {
  var allowsMutationsForCurrentInventory: Bool {
    true
  }
}

enum VirtualMachineClientError: LocalizedError {
  case virtualMachineNotFound
  case daemonResponseInvalid
  case outputPathRequired

  var errorDescription: String? {
    switch self {
    case .virtualMachineNotFound:
      return "The selected virtual machine could not be found."
    case .daemonResponseInvalid:
      return "The daemon returned a response that the app could not understand."
    case .outputPathRequired:
      return "Choose an output folder for this metadata artifact."
    }
  }
}

struct DaemonEndpoint: Equatable {
  var socketPath: String

  static var local: DaemonEndpoint {
    DaemonEndpoint(socketPath: defaultSocketPath())
  }

  static func defaultSocketPath(
    environment: [String: String] = ProcessInfo.processInfo.environment
  ) -> String {
    let home = environment["BRIDGEVM_HOME"].flatMap { nonEmptyPath($0) }
      ?? environment["HOME"].flatMap { nonEmptyPath($0).map { "\($0)/.bridgevm" } }
      ?? ".bridgevm"
    return "\(home)/run/bridgevmd.sock"
  }

  private static func nonEmptyPath(_ path: String) -> String? {
    let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? nil : trimmed
  }
}

struct StoreDoctorReport: Equatable {
  var storeRoot: String
  var vmsDir: String
  var status: String
  var source: String

  var isReady: Bool {
    status.localizedCaseInsensitiveCompare("OK") == .orderedSame
  }
}

final class DaemonVirtualMachineClient: VirtualMachineClient, VirtualMachineClientSourceProviding {
  private let endpoint: DaemonEndpoint
  private let transport: DaemonTransport
  private var namesByID: [VirtualMachine.ID: String] = [:]

  init(
    endpoint: DaemonEndpoint = .local,
    transport: DaemonTransport? = nil
  ) {
    self.endpoint = endpoint
    self.transport = transport ?? UnixSocketNDJSONTransport(endpoint: endpoint)
  }

  var sourceTitle: String {
    "bridgevmd"
  }

  func inspectStoreDoctor() async throws -> StoreDoctorReport {
    let response = try await transport.send(
      DaemonStoreDoctorRequest(),
      responseType: DaemonStoreDoctorResponse.self
    )
    return StoreDoctorReport(
      storeRoot: response.storeRoot,
      vmsDir: response.vmsDir,
      status: response.status,
      source: sourceTitle
    )
  }

  func listVirtualMachines() async throws -> [VirtualMachine] {
    let request = DaemonListVirtualMachinesRequest()
    let response = try await transport.send(
      request,
      responseType: DaemonListVirtualMachinesResponse.self
    )
    let virtualMachines = response.virtualMachines.map { $0.virtualMachine }
    namesByID = Dictionary(uniqueKeysWithValues: virtualMachines.map { ($0.id, $0.name) })
    return virtualMachines
  }

  func listBootTemplates() async throws -> [BootTemplate] {
    let response = try await transport.send(
      DaemonListBootTemplatesRequest(),
      responseType: DaemonListBootTemplatesResponse.self
    )
    return response.templates.map(\.bootTemplate)
  }

  func inspectBootMediaStatus(on id: VirtualMachine.ID) async throws -> BootMediaStatus {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonInspectBootMediaStatusRequest(name: name),
      responseType: DaemonBootMediaStatusResponse.self
    )
    return response.status.bootMediaStatus
  }

  func inspectReadinessReport(on id: VirtualMachine.ID) async throws -> VMReadinessReport {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonReadinessReportRequest(name: name),
      responseType: DaemonReadinessReportResponse.self
    )
    return response.report.vmReadinessReport
  }

  func importBootMedia(
    sourcePath: String,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaImportMetadata {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonImportBootMediaRequest(name: name, source: sourcePath, kind: kind),
      responseType: DaemonBootMediaImportResponse.self
    )
    return response.`import`.bootMediaImportMetadata
  }

  func verifyBootMedia(
    expectedSHA256: String,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaVerificationMetadata {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonVerifyBootMediaRequest(name: name, expectedSHA256: expectedSHA256, kind: kind),
      responseType: DaemonBootMediaVerificationResponse.self
    )
    return response.verification.bootMediaVerificationMetadata
  }

  func planBootMediaDownload(
    url: String,
    expectedSHA256: String?,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaDownloadPlanMetadata {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonPlanBootMediaDownloadRequest(
        name: name,
        url: url,
        expectedSHA256: expectedSHA256,
        kind: kind
      ),
      responseType: DaemonBootMediaDownloadPlanResponse.self
    )
    return response.plan.bootMediaDownloadPlanMetadata
  }

  func downloadBootMedia(
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaDownloadResultMetadata {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonDownloadBootMediaRequest(name: name, kind: kind),
      responseType: DaemonBootMediaDownloadResponse.self
    )
    return response.download.bootMediaDownloadResultMetadata
  }

  func inspectLifecyclePlan(action: LifecyclePlanAction, on id: VirtualMachine.ID) async throws
    -> LifecyclePlan
  {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonLifecyclePlanRequest(name: name, action: action),
      responseType: DaemonLifecyclePlanResponse.self
    )
    return response.plan.lifecyclePlan
  }

  func inspectOpenPortPlan(
    guestPort: UInt16,
    scheme: String,
    on id: VirtualMachine.ID
  ) async throws -> OpenPortPlan {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonOpenPortRequest(name: name, guest: guestPort, scheme: scheme),
      responseType: DaemonOpenPortResponse.self
    )
    return response.plan.openPortPlan
  }

  func inspectNetworkPlan(on id: VirtualMachine.ID) async throws -> NetworkPlan {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonNetworkPlanRequest(name: name),
      responseType: DaemonNetworkPlanResponse.self
    )
    return response.plan.networkPlan
  }

  func inspectSSHPlan(user: String, on id: VirtualMachine.ID) async throws -> SSHPlan {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonSSHPlanRequest(name: name, user: user),
      responseType: DaemonSSHPlanResponse.self
    )
    return response.plan.sshPlan
  }

  func listPortForwards(on id: VirtualMachine.ID) async throws -> VMPortForwardList {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonListPortsRequest(name: name),
      responseType: DaemonPortForwardsResponse.self
    )
    return response.ports.vmPortForwardList
  }

  func addPortForward(host: UInt16, guest: UInt16, on id: VirtualMachine.ID) async throws
    -> VMPortForwardList
  {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonAddPortRequest(name: name, host: host, guest: guest),
      responseType: DaemonPortForwardsResponse.self
    )
    return response.ports.vmPortForwardList
  }

  func removePortForward(host: UInt16, guest: UInt16, on id: VirtualMachine.ID) async throws
    -> VMPortForwardList
  {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonRemovePortRequest(name: name, host: host, guest: guest),
      responseType: DaemonPortForwardsResponse.self
    )
    return response.ports.vmPortForwardList
  }

  func listSharedFolders(on id: VirtualMachine.ID) async throws -> VMSharedFolderList {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonListSharesRequest(name: name),
      responseType: DaemonSharedFoldersResponse.self
    )
    return response.shares.vmSharedFolderList
  }

  func addSharedFolder(
    named shareName: String,
    hostPath: String,
    readOnly: Bool,
    hostPathToken: String?,
    on id: VirtualMachine.ID
  ) async throws -> VMSharedFolderList {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonAddShareRequest(
        name: name,
        share: shareName,
        hostPath: hostPath,
        readOnly: readOnly,
        hostPathToken: hostPathToken
      ),
      responseType: DaemonSharedFoldersResponse.self
    )
    return response.shares.vmSharedFolderList
  }

  func removeSharedFolder(named shareName: String, on id: VirtualMachine.ID) async throws
    -> VMSharedFolderList
  {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonRemoveShareRequest(name: name, share: shareName),
      responseType: DaemonSharedFoldersResponse.self
    )
    return response.shares.vmSharedFolderList
  }

  func inspectGuestToolsStatus(on id: VirtualMachine.ID) async throws -> GuestToolsStatus {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonGuestToolsStatusRequest(name: name),
      responseType: DaemonGuestToolsStatusResponse.self
    )
    return response.status.guestToolsStatus
  }

  func inspectGuestToolsToken(on id: VirtualMachine.ID) async throws -> GuestToolsToken {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonGuestToolsTokenRequest(name: name),
      responseType: DaemonGuestToolsTokenResponse.self
    )
    return response.token.guestToolsToken
  }

  func inspectGuestToolsLinuxCommand(
    transport commandTransport: GuestToolsLinuxCommandTransport,
    on id: VirtualMachine.ID
  ) async throws -> GuestToolsLinuxCommand {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonGuestToolsLinuxCommandRequest(name: name, transport: commandTransport),
      responseType: DaemonGuestToolsLinuxCommandResponse.self
    )
    return response.command.guestToolsLinuxCommand
  }

  func sendGuestToolsCommand(
    _ command: GuestToolsAgentCommand,
    requestID: String?,
    on id: VirtualMachine.ID
  ) async throws -> GuestToolsCommandDispatch {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonGuestToolsSendCommandRequest(name: name, command: command, requestID: requestID),
      responseType: DaemonGuestToolsCommandResponse.self
    )
    guard let command = response.command else {
      throw VirtualMachineClientError.daemonResponseInvalid
    }
    return command.guestToolsCommandDispatch
  }

  func mountApprovedSharedFolder(named shareName: String, on id: VirtualMachine.ID) async throws
    -> GuestToolsStatus?
  {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    _ = try await transport.send(
      DaemonMountSharedFolderRequest(name: name, shareName: shareName),
      responseType: DaemonMountSharedFolderResponse.self
    )
    let response = try await transport.send(
      DaemonGuestToolsStatusRequest(name: name),
      responseType: DaemonGuestToolsStatusResponse.self
    )
    return response.status.guestToolsStatus
  }

  func unmountApprovedSharedFolder(named shareName: String, on id: VirtualMachine.ID) async throws
    -> GuestToolsStatus?
  {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    _ = try await transport.send(
      DaemonGuestToolsSendCommandRequest(
        name: name,
        command: .unmountShare(name: shareName),
        requestID: nil
      ),
      responseType: DaemonGuestToolsCommandResponse.self
    )
    let response = try await transport.send(
      DaemonGuestToolsStatusRequest(name: name),
      responseType: DaemonGuestToolsStatusResponse.self
    )
    return response.status.guestToolsStatus
  }

  func inspectRunnerStatus(on id: VirtualMachine.ID) async throws -> RunnerStatus? {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonRunnerStatusRequest(name: name),
      responseType: DaemonRunnerStatusResponse.self
    )
    return response.runnerStatus
  }

  func inspectQemuArgs(on id: VirtualMachine.ID) async throws -> QemuLaunchPlan {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonQemuArgsRequest(name: name),
      responseType: DaemonQemuCommandResponse.self
    )
    return response.command.qemuLaunchPlan
  }

  func prepareRun(on id: VirtualMachine.ID) async throws -> RunnerStatus {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonPrepareRunRequest(name: name),
      responseType: DaemonRunnerStatusResponse.self
    )
    guard let metadata = response.runnerStatus else {
      throw VirtualMachineClientError.daemonResponseInvalid
    }
    return metadata
  }

  func inspectSnapshotPreflightStatus(on id: VirtualMachine.ID) async throws
    -> SnapshotPreflightStatus
  {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonSnapshotPreflightStatusRequest(name: name, consistency: .applicationConsistent),
      responseType: DaemonSnapshotPreflightStatusResponse.self
    )
    return response.preflight.snapshotPreflightStatus
  }

  func listSnapshots(on id: VirtualMachine.ID) async throws -> [VMSnapshot] {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonListSnapshotsRequest(vm: name),
      responseType: DaemonSnapshotListResponse.self
    )
    return response.snapshots.map(\.vmSnapshot)
  }

  func inspectSnapshotChain(on id: VirtualMachine.ID) async throws -> VMSnapshotChain {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonSnapshotChainRequest(vm: name),
      responseType: DaemonSnapshotChainResponse.self
    )
    return response.chain.vmSnapshotChain
  }

  func createSnapshot(named snapshotName: String, kind: VMSnapshotKind, on id: VirtualMachine.ID)
    async throws -> VMSnapshot
  {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonCreateSnapshotRequest(vm: name, name: snapshotName, kind: kind),
      responseType: DaemonSnapshotCreatedResponse.self
    )
    return response.snapshot.vmSnapshot
  }

  func createSnapshotDisk(named snapshotName: String, on id: VirtualMachine.ID) async throws
    -> VMSnapshotDiskCreation
  {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonCreateSnapshotDiskRequest(vm: name, name: snapshotName),
      responseType: DaemonSnapshotDiskCreatedResponse.self
    )
    return response.metadata.vmSnapshotDiskCreation
  }

  func preparePrimaryDisk(on id: VirtualMachine.ID) async throws -> DiskPreparation {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonPrepareDiskRequest(name: name),
      responseType: DaemonDiskPreparedResponse.self
    )
    return response.metadata.diskPreparation
  }

  func createPrimaryDisk(on id: VirtualMachine.ID) async throws -> VMDiskCreation {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonCreateDiskRequest(name: name),
      responseType: DaemonDiskCreatedResponse.self
    )
    return response.metadata.vmDiskCreation
  }

  func inspectPrimaryDisk(on id: VirtualMachine.ID) async throws -> VMDiskInspection {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonInspectDiskRequest(name: name),
      responseType: DaemonDiskInspectedResponse.self
    )
    return response.metadata.vmDiskInspection
  }

  func verifyActiveDisk(on id: VirtualMachine.ID) async throws -> VMDiskVerification {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonVerifyDiskRequest(name: name),
      responseType: DaemonDiskVerifiedResponse.self
    )
    return response.metadata.vmDiskVerification
  }

  func compactActiveDisk(on id: VirtualMachine.ID) async throws -> VMDiskCompaction {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonCompactDiskRequest(name: name),
      responseType: DaemonDiskCompactedResponse.self
    )
    return response.metadata.vmDiskCompaction
  }

  func repairMetadata(on id: VirtualMachine.ID) async throws -> VMMetadataRepair {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonRepairMetadataRequest(name: name),
      responseType: DaemonMetadataRepairedResponse.self
    )
    return response.repair.vmMetadataRepair
  }

  func migrateManifest(on id: VirtualMachine.ID, dryRun: Bool) async throws -> VMManifestMigration {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonMigrateManifestRequest(name: name, dryRun: dryRun),
      responseType: DaemonManifestMigratedResponse.self
    )
    return response.migration.vmManifestMigration
  }

  func restoreSnapshot(named snapshotName: String, on id: VirtualMachine.ID) async throws
    -> SnapshotRestoreResult
  {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonRestoreSnapshotRequest(vm: name, name: snapshotName),
      responseType: DaemonSnapshotRestoredResponse.self
    )
    return response.restore.snapshotRestoreResult
  }

  func executeApplicationConsistentSnapshot(
    named snapshotName: String,
    freezeTimeoutMillis: UInt64?,
    on id: VirtualMachine.ID
  ) async throws -> ApplicationConsistentSnapshotExecution {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonExecuteApplicationConsistentSnapshotRequest(
        vm: name,
        name: snapshotName,
        freezeTimeoutMillis: freezeTimeoutMillis
      ),
      responseType: DaemonApplicationConsistentSnapshotExecutionResponse.self
    )
    return response.execution.applicationConsistentSnapshotExecution
  }

  func reapplyRuntimeResources(
    visibility: RuntimeResourceVisibility,
    on id: VirtualMachine.ID
  ) async throws -> RuntimeResourcePolicy {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonReapplyRuntimeResourcesRequest(name: name, visibility: visibility),
      responseType: DaemonRuntimeResourcePolicyResponse.self
    )
    return response.policy.runtimeResourcePolicy
  }

  func createDiagnosticBundle(output: String?, on id: VirtualMachine.ID) async throws
    -> DiagnosticBundle
  {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }
    guard let output, !output.isEmpty else {
      throw VirtualMachineClientError.outputPathRequired
    }

    let response = try await transport.send(
      DaemonCreateDiagnosticBundleRequest(name: name, output: output),
      responseType: DaemonDiagnosticBundleResponse.self
    )
    return response.bundle.diagnosticBundle
  }

  func createPerformanceBaseline(output: String?, on id: VirtualMachine.ID) async throws
    -> PerformanceBaseline
  {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }
    guard let output, !output.isEmpty else {
      throw VirtualMachineClientError.outputPathRequired
    }

    let response = try await transport.send(
      DaemonCreatePerformanceBaselineRequest(name: name, output: output),
      responseType: DaemonPerformanceBaselineResponse.self
    )
    return response.baseline.performanceBaseline
  }

  func createPerformanceSample(
    output: String?,
    artifactBytes: UInt64,
    iterations: UInt16,
    sync: Bool,
    on id: VirtualMachine.ID
  ) async throws -> PerformanceSample {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }
    guard let output, !output.isEmpty else {
      throw VirtualMachineClientError.outputPathRequired
    }

    let response = try await transport.send(
      DaemonCreatePerformanceSampleRequest(
        name: name,
        output: output,
        artifactBytes: artifactBytes,
        iterations: iterations,
        sync: sync
      ),
      responseType: DaemonPerformanceSampleResponse.self
    )
    return response.sample.performanceSample
  }

  func inspectQMPStatus(on id: VirtualMachine.ID) async throws -> QMPStatus {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonQMPStatusRequest(name: name),
      responseType: DaemonQMPStatusResponse.self
    )
    return response.status.qmpStatus
  }

  func viewLogs(kind: VMLogKind, bytes: UInt64?, on id: VirtualMachine.ID) async throws
    -> VMLogView
  {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonViewLogsRequest(name: name, kind: kind, maxBytes: bytes),
      responseType: DaemonLogsViewedResponse.self
    )
    return response.log.vmLogView
  }

  func exportVirtualMachine(on id: VirtualMachine.ID, output: String) async throws
    -> VMExportMetadata
  {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }
    guard !output.isEmpty else {
      throw VirtualMachineClientError.outputPathRequired
    }

    let response = try await transport.send(
      DaemonExportVirtualMachineRequest(name: name, output: output),
      responseType: DaemonExportVirtualMachineResponse.self
    )
    return response.export.vmExportMetadata
  }

  func importVirtualMachine(input: String, name: String?) async throws -> VMImportMetadata {
    guard !input.isEmpty else {
      throw VirtualMachineClientError.outputPathRequired
    }

    let response = try await transport.send(
      DaemonImportVirtualMachineRequest(input: input, name: name),
      responseType: DaemonImportVirtualMachineResponse.self
    )
    return response.`import`.vmImportMetadata
  }

  func createVirtualMachine(_ request: CreateVirtualMachineRequest) async throws -> VirtualMachine {
    let response = try await transport.send(
      DaemonCreateVirtualMachineRequest(createRequest: request),
      responseType: DaemonVirtualMachineResponse.self
    )
    let virtualMachine = response.virtualMachine.virtualMachine
    namesByID[virtualMachine.id] = virtualMachine.name
    return virtualMachine
  }

  func recommendMode(for choice: GuestChoice) async throws -> ModeRecommendation {
    let response = try await transport.send(
      DaemonRecommendModeRequest(choice: choice),
      responseType: DaemonModeRecommendationResponse.self
    )
    return response.recommendation
  }

  func cloneVirtualMachine(on id: VirtualMachine.ID, newName: String, linked: Bool) async throws
    -> CloneVirtualMachineMetadata
  {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonCloneVirtualMachineRequest(name: name, newName: newName, linked: linked),
      responseType: DaemonCloneVirtualMachineResponse.self
    )
    return response.clone.cloneVirtualMachineMetadata
  }

  func deleteVirtualMachine(on id: VirtualMachine.ID) async throws -> VMDeletionMetadata {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let response = try await transport.send(
      DaemonDeleteVirtualMachineRequest(name: name, metadataOnly: true),
      responseType: DaemonDeleteVirtualMachineResponse.self
    )
    namesByID.removeValue(forKey: id)
    return response.vmDeletionMetadata
  }

  func perform(_ action: VirtualMachineAction, on id: VirtualMachine.ID) async throws
    -> VMActionResult
  {
    guard let name = namesByID[id] else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    switch action {
    case .start:
      try await runBackend(name: name, spawn: true)
    case .resume:
      try await resume(name: name)
    case .pause:
      try await suspend(name: name)
    case .stop:
      try await stop(name: name)
    case .restart:
      try await stop(name: name)
      try await runBackend(name: name, spawn: true)
    }

    let refreshed = try await listVirtualMachines()
    guard let virtualMachine = refreshed.first(where: { $0.name == name }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }
    return VMActionResult(
      virtualMachine: virtualMachine,
      message: "\(virtualMachine.name) \(action.pastTenseMessage)."
    )
  }

  private func stop(name: String) async throws {
    _ = try await transport.send(
      DaemonStopVirtualMachineRequest(name: name),
      responseType: DaemonRunnerStatusResponse.self
    )
  }

  private func runBackend(name: String, spawn: Bool) async throws {
    _ = try await transport.send(
      DaemonRunBackendRequest(name: name, spawn: spawn),
      responseType: DaemonRunnerStatusResponse.self
    )
  }

  private func suspend(name: String) async throws {
    _ = try await transport.send(
      DaemonSuspendBackendRequest(name: name),
      responseType: DaemonRunnerStatusResponse.self
    )
  }

  private func resume(name: String) async throws {
    _ = try await transport.send(
      DaemonResumeBackendRequest(name: name),
      responseType: DaemonRunnerStatusResponse.self
    )
  }
}

final class FallbackVirtualMachineClient: VirtualMachineClient, VirtualMachineClientSourceProviding
{
  private let primary: VirtualMachineClient
  private let fallback: VirtualMachineClient
  private(set) var sourceTitle = "bridgevmd"
  private(set) var allowsMutationsForCurrentInventory = true

  init(primary: VirtualMachineClient, fallback: VirtualMachineClient) {
    self.primary = primary
    self.fallback = fallback
  }

  private func markPrimarySource() {
    sourceTitle = (primary as? VirtualMachineClientSourceProviding)?.sourceTitle ?? "bridgevmd"
    allowsMutationsForCurrentInventory = true
  }

  private func markFallbackSource() {
    sourceTitle =
      (fallback as? VirtualMachineClientSourceProviding)?.sourceTitle ?? "Fallback inventory"
    allowsMutationsForCurrentInventory = false
  }

  private func runPrimaryMutation<Value>(
    _ operation: (VirtualMachineClient) async throws -> Value
  ) async throws -> Value {
    let value = try await operation(primary)
    markPrimarySource()
    return value
  }

  func listVirtualMachines() async throws -> [VirtualMachine] {
    do {
      let virtualMachines = try await primary.listVirtualMachines()
      markPrimarySource()
      return virtualMachines
    } catch {
      let virtualMachines = try await fallback.listVirtualMachines()
      markFallbackSource()
      return virtualMachines
    }
  }

  func inspectStoreDoctor() async throws -> StoreDoctorReport {
    do {
      let report = try await primary.inspectStoreDoctor()
      markPrimarySource()
      return StoreDoctorReport(
        storeRoot: report.storeRoot,
        vmsDir: report.vmsDir,
        status: report.status,
        source: sourceTitle
      )
    } catch {
      let report = try await fallback.inspectStoreDoctor()
      markFallbackSource()
      return StoreDoctorReport(
        storeRoot: report.storeRoot,
        vmsDir: report.vmsDir,
        status: report.status,
        source: sourceTitle
      )
    }
  }

  func listBootTemplates() async throws -> [BootTemplate] {
    do {
      let templates = try await primary.listBootTemplates()
      markPrimarySource()
      return templates
    } catch {
      let templates = try await fallback.listBootTemplates()
      markFallbackSource()
      return templates
    }
  }

  func inspectBootMediaStatus(on id: VirtualMachine.ID) async throws -> BootMediaStatus {
    do {
      let status = try await primary.inspectBootMediaStatus(on: id)
      markPrimarySource()
      return status
    } catch {
      let status = try await fallback.inspectBootMediaStatus(on: id)
      markFallbackSource()
      return status
    }
  }

  func inspectReadinessReport(on id: VirtualMachine.ID) async throws -> VMReadinessReport {
    do {
      let report = try await primary.inspectReadinessReport(on: id)
      markPrimarySource()
      return report
    } catch {
      let report = try await fallback.inspectReadinessReport(on: id)
      markFallbackSource()
      return report
    }
  }

  func importBootMedia(
    sourcePath: String,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaImportMetadata {
    try await runPrimaryMutation {
      try await $0.importBootMedia(sourcePath: sourcePath, kind: kind, on: id)
    }
  }

  func verifyBootMedia(
    expectedSHA256: String,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaVerificationMetadata {
    try await runPrimaryMutation {
      try await $0.verifyBootMedia(
        expectedSHA256: expectedSHA256, kind: kind, on: id)
    }
  }

  func planBootMediaDownload(
    url: String,
    expectedSHA256: String?,
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaDownloadPlanMetadata {
    try await runPrimaryMutation {
      try await $0.planBootMediaDownload(
        url: url,
        expectedSHA256: expectedSHA256,
        kind: kind,
        on: id
      )
    }
  }

  func downloadBootMedia(
    kind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaDownloadResultMetadata {
    try await runPrimaryMutation {
      try await $0.downloadBootMedia(kind: kind, on: id)
    }
  }

  func inspectLifecyclePlan(action: LifecyclePlanAction, on id: VirtualMachine.ID) async throws
    -> LifecyclePlan
  {
    do {
      let plan = try await primary.inspectLifecyclePlan(action: action, on: id)
      markPrimarySource()
      return plan
    } catch {
      let plan = try await fallback.inspectLifecyclePlan(action: action, on: id)
      markFallbackSource()
      return plan
    }
  }

  func inspectOpenPortPlan(
    guestPort: UInt16,
    scheme: String,
    on id: VirtualMachine.ID
  ) async throws -> OpenPortPlan {
    do {
      let plan = try await primary.inspectOpenPortPlan(
        guestPort: guestPort,
        scheme: scheme,
        on: id
      )
      markPrimarySource()
      return plan
    } catch {
      let plan = try await fallback.inspectOpenPortPlan(
        guestPort: guestPort,
        scheme: scheme,
        on: id
      )
      markFallbackSource()
      return plan
    }
  }

  func inspectNetworkPlan(on id: VirtualMachine.ID) async throws -> NetworkPlan {
    do {
      let plan = try await primary.inspectNetworkPlan(on: id)
      markPrimarySource()
      return plan
    } catch {
      let plan = try await fallback.inspectNetworkPlan(on: id)
      markFallbackSource()
      return plan
    }
  }

  func inspectSSHPlan(user: String, on id: VirtualMachine.ID) async throws -> SSHPlan {
    do {
      let plan = try await primary.inspectSSHPlan(user: user, on: id)
      markPrimarySource()
      return plan
    } catch {
      let plan = try await fallback.inspectSSHPlan(user: user, on: id)
      markFallbackSource()
      return plan
    }
  }

  func listPortForwards(on id: VirtualMachine.ID) async throws -> VMPortForwardList {
    do {
      let ports = try await primary.listPortForwards(on: id)
      markPrimarySource()
      return ports
    } catch {
      let ports = try await fallback.listPortForwards(on: id)
      markFallbackSource()
      return ports
    }
  }

  func addPortForward(host: UInt16, guest: UInt16, on id: VirtualMachine.ID) async throws
    -> VMPortForwardList
  {
    try await runPrimaryMutation {
      try await $0.addPortForward(host: host, guest: guest, on: id)
    }
  }

  func removePortForward(host: UInt16, guest: UInt16, on id: VirtualMachine.ID) async throws
    -> VMPortForwardList
  {
    try await runPrimaryMutation {
      try await $0.removePortForward(host: host, guest: guest, on: id)
    }
  }

  func inspectGuestToolsStatus(on id: VirtualMachine.ID) async throws -> GuestToolsStatus {
    do {
      let status = try await primary.inspectGuestToolsStatus(on: id)
      markPrimarySource()
      return status
    } catch {
      let status = try await fallback.inspectGuestToolsStatus(on: id)
      markFallbackSource()
      return status
    }
  }

  func inspectGuestToolsToken(on id: VirtualMachine.ID) async throws -> GuestToolsToken {
    do {
      let token = try await primary.inspectGuestToolsToken(on: id)
      markPrimarySource()
      return token
    } catch {
      let token = try await fallback.inspectGuestToolsToken(on: id)
      markFallbackSource()
      return token
    }
  }

  func inspectGuestToolsLinuxCommand(
    transport: GuestToolsLinuxCommandTransport,
    on id: VirtualMachine.ID
  ) async throws -> GuestToolsLinuxCommand {
    do {
      let command = try await primary.inspectGuestToolsLinuxCommand(transport: transport, on: id)
      markPrimarySource()
      return command
    } catch {
      let command = try await fallback.inspectGuestToolsLinuxCommand(transport: transport, on: id)
      markFallbackSource()
      return command
    }
  }

  func listSharedFolders(on id: VirtualMachine.ID) async throws -> VMSharedFolderList {
    do {
      let shares = try await primary.listSharedFolders(on: id)
      markPrimarySource()
      return shares
    } catch {
      let shares = try await fallback.listSharedFolders(on: id)
      markFallbackSource()
      return shares
    }
  }

  func addSharedFolder(
    named shareName: String,
    hostPath: String,
    readOnly: Bool,
    hostPathToken: String?,
    on id: VirtualMachine.ID
  ) async throws -> VMSharedFolderList {
    try await runPrimaryMutation {
      try await $0.addSharedFolder(
        named: shareName,
        hostPath: hostPath,
        readOnly: readOnly,
        hostPathToken: hostPathToken,
        on: id
      )
    }
  }

  func removeSharedFolder(named shareName: String, on id: VirtualMachine.ID) async throws
    -> VMSharedFolderList
  {
    try await runPrimaryMutation {
      try await $0.removeSharedFolder(named: shareName, on: id)
    }
  }

  func mountApprovedSharedFolder(named shareName: String, on id: VirtualMachine.ID) async throws
    -> GuestToolsStatus?
  {
    try await runPrimaryMutation {
      try await $0.mountApprovedSharedFolder(named: shareName, on: id)
    }
  }

  func unmountApprovedSharedFolder(named shareName: String, on id: VirtualMachine.ID) async throws
    -> GuestToolsStatus?
  {
    try await runPrimaryMutation {
      try await $0.unmountApprovedSharedFolder(named: shareName, on: id)
    }
  }

  func sendGuestToolsCommand(
    _ command: GuestToolsAgentCommand,
    requestID: String?,
    on id: VirtualMachine.ID
  ) async throws -> GuestToolsCommandDispatch {
    try await runPrimaryMutation {
      try await $0.sendGuestToolsCommand(command, requestID: requestID, on: id)
    }
  }

  func inspectRunnerStatus(on id: VirtualMachine.ID) async throws -> RunnerStatus? {
    do {
      let status = try await primary.inspectRunnerStatus(on: id)
      markPrimarySource()
      return status
    } catch {
      let status = try await fallback.inspectRunnerStatus(on: id)
      markFallbackSource()
      return status
    }
  }

  func inspectQemuArgs(on id: VirtualMachine.ID) async throws -> QemuLaunchPlan {
    do {
      let plan = try await primary.inspectQemuArgs(on: id)
      markPrimarySource()
      return plan
    } catch {
      let plan = try await fallback.inspectQemuArgs(on: id)
      markFallbackSource()
      return plan
    }
  }

  func prepareRun(on id: VirtualMachine.ID) async throws -> RunnerStatus {
    try await runPrimaryMutation {
      try await $0.prepareRun(on: id)
    }
  }

  func inspectSnapshotPreflightStatus(on id: VirtualMachine.ID) async throws
    -> SnapshotPreflightStatus
  {
    do {
      let status = try await primary.inspectSnapshotPreflightStatus(on: id)
      markPrimarySource()
      return status
    } catch {
      let status = try await fallback.inspectSnapshotPreflightStatus(on: id)
      markFallbackSource()
      return status
    }
  }

  func listSnapshots(on id: VirtualMachine.ID) async throws -> [VMSnapshot] {
    do {
      let snapshots = try await primary.listSnapshots(on: id)
      markPrimarySource()
      return snapshots
    } catch {
      let snapshots = try await fallback.listSnapshots(on: id)
      markFallbackSource()
      return snapshots
    }
  }

  func inspectSnapshotChain(on id: VirtualMachine.ID) async throws -> VMSnapshotChain {
    do {
      let chain = try await primary.inspectSnapshotChain(on: id)
      markPrimarySource()
      return chain
    } catch {
      let chain = try await fallback.inspectSnapshotChain(on: id)
      markFallbackSource()
      return chain
    }
  }

  func createSnapshotDisk(named snapshotName: String, on id: VirtualMachine.ID) async throws
    -> VMSnapshotDiskCreation
  {
    try await runPrimaryMutation {
      try await $0.createSnapshotDisk(named: snapshotName, on: id)
    }
  }

  func createSnapshot(named snapshotName: String, kind: VMSnapshotKind, on id: VirtualMachine.ID)
    async throws -> VMSnapshot
  {
    try await runPrimaryMutation {
      try await $0.createSnapshot(named: snapshotName, kind: kind, on: id)
    }
  }

  func preparePrimaryDisk(on id: VirtualMachine.ID) async throws -> DiskPreparation {
    try await runPrimaryMutation {
      try await $0.preparePrimaryDisk(on: id)
    }
  }

  func createPrimaryDisk(on id: VirtualMachine.ID) async throws -> VMDiskCreation {
    try await runPrimaryMutation {
      try await $0.createPrimaryDisk(on: id)
    }
  }

  func inspectPrimaryDisk(on id: VirtualMachine.ID) async throws -> VMDiskInspection {
    do {
      let inspection = try await primary.inspectPrimaryDisk(on: id)
      markPrimarySource()
      return inspection
    } catch {
      let inspection = try await fallback.inspectPrimaryDisk(on: id)
      markFallbackSource()
      return inspection
    }
  }

  func restoreSnapshot(named snapshotName: String, on id: VirtualMachine.ID) async throws
    -> SnapshotRestoreResult
  {
    try await runPrimaryMutation {
      try await $0.restoreSnapshot(named: snapshotName, on: id)
    }
  }

  func verifyActiveDisk(on id: VirtualMachine.ID) async throws -> VMDiskVerification {
    do {
      let verification = try await primary.verifyActiveDisk(on: id)
      markPrimarySource()
      return verification
    } catch {
      let verification = try await fallback.verifyActiveDisk(on: id)
      markFallbackSource()
      return verification
    }
  }

  func compactActiveDisk(on id: VirtualMachine.ID) async throws -> VMDiskCompaction {
    try await runPrimaryMutation {
      try await $0.compactActiveDisk(on: id)
    }
  }

  func repairMetadata(on id: VirtualMachine.ID) async throws -> VMMetadataRepair {
    try await runPrimaryMutation {
      try await $0.repairMetadata(on: id)
    }
  }

  func migrateManifest(on id: VirtualMachine.ID, dryRun: Bool) async throws -> VMManifestMigration {
    try await runPrimaryMutation {
      try await $0.migrateManifest(on: id, dryRun: dryRun)
    }
  }

  func executeApplicationConsistentSnapshot(
    named snapshotName: String,
    freezeTimeoutMillis: UInt64?,
    on id: VirtualMachine.ID
  ) async throws -> ApplicationConsistentSnapshotExecution {
    try await runPrimaryMutation {
      try await $0.executeApplicationConsistentSnapshot(
        named: snapshotName,
        freezeTimeoutMillis: freezeTimeoutMillis,
        on: id
      )
    }
  }

  func reapplyRuntimeResources(
    visibility: RuntimeResourceVisibility,
    on id: VirtualMachine.ID
  ) async throws -> RuntimeResourcePolicy {
    try await runPrimaryMutation {
      try await $0.reapplyRuntimeResources(visibility: visibility, on: id)
    }
  }

  func createDiagnosticBundle(output: String?, on id: VirtualMachine.ID) async throws
    -> DiagnosticBundle
  {
    try await runPrimaryMutation {
      try await $0.createDiagnosticBundle(output: output, on: id)
    }
  }

  func createPerformanceBaseline(output: String?, on id: VirtualMachine.ID) async throws
    -> PerformanceBaseline
  {
    try await runPrimaryMutation {
      try await $0.createPerformanceBaseline(output: output, on: id)
    }
  }

  func createPerformanceSample(
    output: String?,
    artifactBytes: UInt64,
    iterations: UInt16,
    sync: Bool,
    on id: VirtualMachine.ID
  ) async throws -> PerformanceSample {
    try await runPrimaryMutation {
      try await $0.createPerformanceSample(
        output: output,
        artifactBytes: artifactBytes,
        iterations: iterations,
        sync: sync,
        on: id
      )
    }
  }

  func inspectQMPStatus(on id: VirtualMachine.ID) async throws -> QMPStatus {
    do {
      let status = try await primary.inspectQMPStatus(on: id)
      markPrimarySource()
      return status
    } catch {
      let status = try await fallback.inspectQMPStatus(on: id)
      markFallbackSource()
      return status
    }
  }

  func viewLogs(kind: VMLogKind, bytes: UInt64?, on id: VirtualMachine.ID) async throws
    -> VMLogView
  {
    do {
      let log = try await primary.viewLogs(kind: kind, bytes: bytes, on: id)
      markPrimarySource()
      return log
    } catch {
      let log = try await fallback.viewLogs(kind: kind, bytes: bytes, on: id)
      markFallbackSource()
      return log
    }
  }

  func exportVirtualMachine(on id: VirtualMachine.ID, output: String) async throws
    -> VMExportMetadata
  {
    try await runPrimaryMutation {
      try await $0.exportVirtualMachine(on: id, output: output)
    }
  }

  func importVirtualMachine(input: String, name: String?) async throws -> VMImportMetadata {
    try await runPrimaryMutation {
      try await $0.importVirtualMachine(input: input, name: name)
    }
  }

  func createVirtualMachine(_ request: CreateVirtualMachineRequest) async throws -> VirtualMachine {
    try await runPrimaryMutation {
      try await $0.createVirtualMachine(request)
    }
  }

  func recommendMode(for choice: GuestChoice) async throws -> ModeRecommendation {
    do {
      let recommendation = try await primary.recommendMode(for: choice)
      markPrimarySource()
      return recommendation
    } catch {
      let recommendation = try await fallback.recommendMode(for: choice)
      markFallbackSource()
      return recommendation
    }
  }

  func cloneVirtualMachine(on id: VirtualMachine.ID, newName: String, linked: Bool) async throws
    -> CloneVirtualMachineMetadata
  {
    try await runPrimaryMutation {
      try await $0.cloneVirtualMachine(on: id, newName: newName, linked: linked)
    }
  }

  func deleteVirtualMachine(on id: VirtualMachine.ID) async throws -> VMDeletionMetadata {
    try await runPrimaryMutation {
      try await $0.deleteVirtualMachine(on: id)
    }
  }

  func perform(_ action: VirtualMachineAction, on id: VirtualMachine.ID) async throws
    -> VMActionResult
  {
    try await runPrimaryMutation {
      try await $0.perform(action, on: id)
    }
  }
}

protocol DaemonTransport {
  func send<Request: Encodable, Response: Decodable>(
    _ request: Request,
    responseType: Response.Type
  ) async throws -> Response
}

enum DaemonRequestTimeoutCategory: Equatable {
  case quick
  case lifecycleAction
  case guestToolsCommand
  case mediaOperation
  case diskOperation
  case snapshotOperation
  case archiveOperation
  case diagnosticsOperation

  var nanoseconds: UInt64 {
    switch self {
    case .quick:
      return 2_000_000_000
    case .lifecycleAction, .guestToolsCommand:
      return 15_000_000_000
    case .mediaOperation, .diskOperation, .snapshotOperation, .diagnosticsOperation:
      return 120_000_000_000
    case .archiveOperation:
      return 600_000_000_000
    }
  }
}

protocol DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { get }
}

extension DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutNanoseconds: UInt64 {
    daemonRequestTimeoutCategory.nanoseconds
  }
}

enum DaemonTransportError: LocalizedError {
  case connectionFailed
  case daemonError(String)
  case responseClosed
  case responseEncodingInvalid
  case requestTimedOut

  var errorDescription: String? {
    switch self {
    case .connectionFailed:
      return "The app could not connect to the daemon socket."
    case .daemonError(let message):
      return message
    case .responseClosed:
      return "The daemon closed the socket before returning a response."
    case .responseEncodingInvalid:
      return "The daemon response was not valid UTF-8 newline-delimited JSON."
    case .requestTimedOut:
      return "The daemon request timed out."
    }
  }
}

final class UnixSocketNDJSONTransport: DaemonTransport {
  private let endpoint: DaemonEndpoint
  private let encoder = JSONEncoder()
  private let decoder = JSONDecoder()

  init(endpoint: DaemonEndpoint) {
    self.endpoint = endpoint
  }

  func send<Request: Encodable, Response: Decodable>(
    _ request: Request,
    responseType: Response.Type
  ) async throws -> Response {
    let timeoutNanoseconds = Self.timeoutNanoseconds(for: request)

    return try await withThrowingTaskGroup(of: Response.self) { group in
      group.addTask {
        try await self.sendWithoutTimeout(request, responseType: responseType)
      }
      group.addTask {
        try await Task.sleep(nanoseconds: timeoutNanoseconds)
        throw DaemonTransportError.requestTimedOut
      }

      guard let response = try await group.next() else {
        throw DaemonTransportError.responseClosed
      }
      group.cancelAll()
      return response
    }
  }

  static func timeoutNanoseconds<Request: Encodable>(for request: Request) -> UInt64 {
    guard let timeoutProviding = request as? DaemonRequestTimeoutProviding else {
      return DaemonRequestTimeoutCategory.quick.nanoseconds
    }

    return timeoutProviding.daemonRequestTimeoutNanoseconds
  }

  static func decodeResponse<Response: Decodable>(
    _ data: Data,
    as responseType: Response.Type,
    decoder: JSONDecoder = JSONDecoder()
  ) throws -> Response {
    if let daemonError = try? decoder.decode(DaemonErrorResponse.self, from: data),
      daemonError.type == "error"
    {
      throw DaemonTransportError.daemonError(daemonError.message)
    }

    return try decoder.decode(responseType, from: data)
  }

  private func sendWithoutTimeout<Request: Encodable, Response: Decodable>(
    _ request: Request,
    responseType: Response.Type
  ) async throws -> Response {
    let connection = NWConnection(
      to: .unix(path: endpoint.socketPath),
      using: .tcp
    )
    defer { connection.cancel() }

    try await connection.startAndWait()

    var requestData = try encoder.encode(request)
    requestData.append(0x0A)

    try await connection.sendAndWait(requestData)
    let responseData = try await connection.receiveLine()
    return try Self.decodeResponse(responseData, as: responseType, decoder: decoder)
  }
}

extension NWConnection {
  fileprivate func startAndWait() async throws {
    let resume = ContinuationResumeBox<Void>()

    stateUpdateHandler = { state in
      switch state {
      case .ready:
        resume.succeed(())
      case .failed(let error):
        resume.fail(error)
      case .cancelled:
        resume.fail(DaemonTransportError.connectionFailed)
      default:
        break
      }
    }

    start(queue: .global(qos: .userInitiated))

    return try await withTaskCancellationHandler {
      try await withCheckedThrowingContinuation { continuation in
        resume.set(continuation)
      }
    } onCancel: {
      cancel()
      resume.fail(DaemonTransportError.connectionFailed)
    }
  }

  fileprivate func sendAndWait(_ data: Data) async throws {
    let resume = ContinuationResumeBox<Void>()

    try await withTaskCancellationHandler {
      try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
        resume.set(continuation)
        send(
          content: data,
          completion: .contentProcessed { error in
            if let error {
              resume.fail(error)
            } else {
              resume.succeed(())
            }
          })
      }
    } onCancel: {
      cancel()
      resume.fail(DaemonTransportError.connectionFailed)
    }
  }

  fileprivate func receiveLine() async throws -> Data {
    var buffer = Data()

    while true {
      let chunk = try await receiveChunk()

      if chunk.isEmpty {
        throw DaemonTransportError.responseClosed
      }

      if let newlineIndex = chunk.firstIndex(of: 0x0A) {
        buffer.append(chunk[..<newlineIndex])
        return buffer
      }

      buffer.append(chunk)
    }
  }

  private func receiveChunk() async throws -> Data {
    let resume = ContinuationResumeBox<Data>()

    return try await withTaskCancellationHandler {
      try await withCheckedThrowingContinuation { continuation in
        resume.set(continuation)
        receive(minimumIncompleteLength: 1, maximumLength: 64 * 1024) { data, _, isComplete, error in
          if let error {
            resume.fail(error)
          } else if let data, !data.isEmpty {
            resume.succeed(data)
          } else if isComplete {
            resume.succeed(Data())
          } else {
            resume.fail(DaemonTransportError.responseEncodingInvalid)
          }
        }
      }
    } onCancel: {
      cancel()
      resume.fail(DaemonTransportError.connectionFailed)
    }
  }
}

private final class ContinuationResumeBox<Value>: @unchecked Sendable {
  private let lock = NSLock()
  private var continuation: CheckedContinuation<Value, Error>?
  private var result: Result<Value, Error>?

  func set(_ continuation: CheckedContinuation<Value, Error>) {
    let pendingResult: Result<Value, Error>?

    lock.lock()
    if let result {
      pendingResult = result
    } else {
      self.continuation = continuation
      pendingResult = nil
    }
    lock.unlock()

    if let pendingResult {
      continuation.resume(with: pendingResult)
    }
  }

  func succeed(_ value: Value) {
    resume(.success(value))
  }

  func fail(_ error: Error) {
    resume(.failure(error))
  }

  private func resume(_ result: Result<Value, Error>) {
    let continuation: CheckedContinuation<Value, Error>?

    lock.lock()
    if let existingContinuation = self.continuation {
      continuation = existingContinuation
      self.continuation = nil
    } else if self.result == nil {
      self.result = result
      continuation = nil
    } else {
      continuation = nil
    }
    lock.unlock()

    continuation?.resume(with: result)
  }
}

struct DaemonListVirtualMachinesRequest: Encodable {
  let type = "list_vms"
}

struct DaemonListBootTemplatesRequest: Encodable {
  let type = "list_templates"
}

struct DaemonInspectBootMediaStatusRequest: Encodable {
  let type = "inspect_boot_media_status"
  var name: String
}

struct DaemonReadinessReportRequest: Encodable {
  let type = "readiness_report"
  var name: String
}

struct DaemonImportBootMediaRequest: Encodable {
  let type = "import_boot_media"
  var name: String
  var source: String
  var kind: BootMediaStatusEntry.Kind?

  enum CodingKeys: String, CodingKey {
    case type
    case name
    case source
    case kind
  }

  func encode(to encoder: Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    try container.encode(type, forKey: .type)
    try container.encode(name, forKey: .name)
    try container.encode(source, forKey: .source)
    if let kind, kind.isImportable {
      try container.encode(kind.rawValue, forKey: .kind)
    } else {
      try container.encodeNil(forKey: .kind)
    }
  }
}

struct DaemonVerifyBootMediaRequest: Encodable {
  let type = "verify_boot_media"
  var name: String
  var expectedSHA256: String
  var kind: BootMediaStatusEntry.Kind?

  enum CodingKeys: String, CodingKey {
    case type
    case name
    case expectedSHA256 = "expected_sha256"
    case kind
  }

  func encode(to encoder: Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    try container.encode(type, forKey: .type)
    try container.encode(name, forKey: .name)
    try container.encode(expectedSHA256, forKey: .expectedSHA256)
    if let kind, kind.isImportable {
      try container.encode(kind.rawValue, forKey: .kind)
    } else {
      try container.encodeNil(forKey: .kind)
    }
  }
}

struct DaemonPlanBootMediaDownloadRequest: Encodable {
  let type = "plan_boot_media_download"
  var name: String
  var url: String
  var expectedSHA256: String?
  var kind: BootMediaStatusEntry.Kind?

  enum CodingKeys: String, CodingKey {
    case type
    case name
    case url
    case expectedSHA256 = "expected_sha256"
    case kind
  }

  func encode(to encoder: Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    try container.encode(type, forKey: .type)
    try container.encode(name, forKey: .name)
    try container.encode(url, forKey: .url)
    if let expectedSHA256 {
      try container.encode(expectedSHA256, forKey: .expectedSHA256)
    } else {
      try container.encodeNil(forKey: .expectedSHA256)
    }
    if let kind, kind.isImportable {
      try container.encode(kind.rawValue, forKey: .kind)
    } else {
      try container.encodeNil(forKey: .kind)
    }
  }
}

struct DaemonDownloadBootMediaRequest: Encodable {
  let type = "download_boot_media"
  var name: String
  var kind: BootMediaStatusEntry.Kind?

  enum CodingKeys: String, CodingKey {
    case type
    case name
    case kind
  }

  func encode(to encoder: Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    try container.encode(type, forKey: .type)
    try container.encode(name, forKey: .name)
    if let kind, kind.isImportable {
      try container.encode(kind.rawValue, forKey: .kind)
    } else {
      try container.encodeNil(forKey: .kind)
    }
  }
}

struct DaemonGuestToolsStatusRequest: Encodable {
  let type = "guest_tools_status"
  var name: String
}

struct DaemonGuestToolsTokenRequest: Encodable {
  let type = "guest_tools_token"
  var name: String
}

struct DaemonGuestToolsLinuxCommandRequest: Encodable {
  let type = "guest_tools_linux_command"
  var name: String
  var transport: GuestToolsLinuxCommandTransport
  var tokenFile: String?
  var device: String?

  enum CodingKeys: String, CodingKey {
    case type
    case name
    case transport
    case tokenFile = "token_file"
    case device
  }
}

extension GuestToolsLinuxCommandTransport: Encodable {
  func encode(to encoder: Encoder) throws {
    var container = encoder.singleValueContainer()
    try container.encode(rawValue)
  }
}

extension GuestToolsLinuxCommandTransport: Decodable {}

struct DaemonMountSharedFolderRequest: Encodable {
  let type = "guest_tools_mount_approved_share"
  var name: String
  var shareName: String

  enum CodingKeys: String, CodingKey {
    case type
    case name
    case shareName = "share"
  }
}

struct DaemonGuestToolsSendCommandRequest: Encodable {
  let type = "guest_tools_send_command"
  var name: String
  var envelope: DaemonAgentEnvelope

  init(name: String, command: GuestToolsAgentCommand, requestID: String?) {
    self.name = name
    envelope = DaemonAgentEnvelope(message: command, requestID: requestID)
  }
}

struct DaemonAgentEnvelope: Encodable {
  let protocolVersion: UInt16 = 1
  var message: GuestToolsAgentCommand
  var requestID: String?

  enum CodingKeys: String, CodingKey {
    case protocolVersion = "protocol_version"
    case requestID = "request_id"
    case message
  }

  func encode(to encoder: Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    try container.encode(protocolVersion, forKey: .protocolVersion)
    try container.encode(message, forKey: .message)
    try container.encodeIfPresent(requestID, forKey: .requestID)
  }
}

extension GuestToolsAgentCommand: Encodable {
  private struct DynamicCodingKey: CodingKey {
    var stringValue: String
    var intValue: Int?

    init(_ stringValue: String) {
      self.stringValue = stringValue
      intValue = nil
    }

    init?(stringValue: String) {
      self.init(stringValue)
    }

    init?(intValue: Int) {
      return nil
    }
  }

  private enum SetClipboardCodingKeys: String, CodingKey {
    case text
  }

  private enum ResizeDisplayCodingKeys: String, CodingKey {
    case width
    case height
    case scale
  }

  private enum FileDropStartCodingKeys: String, CodingKey {
    case transferID = "transfer_id"
    case fileName = "file_name"
    case sizeBytes = "size_bytes"
  }

  private enum FileDropChunkCodingKeys: String, CodingKey {
    case transferID = "transfer_id"
    case chunkIndex = "chunk_index"
    case dataBase64 = "data_base64"
  }

  private enum FileDropCompleteCodingKeys: String, CodingKey {
    case transferID = "transfer_id"
  }

  private enum IDCommandCodingKeys: String, CodingKey {
    case id
  }

  private enum ShareCommandCodingKeys: String, CodingKey {
    case name
  }

  private enum TimeSyncCodingKeys: String, CodingKey {
    case unixEpochMillis = "unix_epoch_millis"
  }

  func encode(to encoder: Encoder) throws {
    switch self {
    case .setClipboard(let text):
      var container = encoder.container(keyedBy: DynamicCodingKey.self)
      var payload = container.nestedContainer(
        keyedBy: SetClipboardCodingKeys.self,
        forKey: DynamicCodingKey("SetClipboard")
      )
      try payload.encode(text, forKey: .text)
    case .resizeDisplay(let width, let height, let scale):
      var container = encoder.container(keyedBy: DynamicCodingKey.self)
      var payload = container.nestedContainer(
        keyedBy: ResizeDisplayCodingKeys.self,
        forKey: DynamicCodingKey("ResizeDisplay")
      )
      try payload.encode(width, forKey: .width)
      try payload.encode(height, forKey: .height)
      try payload.encode(scale, forKey: .scale)
    case .unmountShare(let name):
      var container = encoder.container(keyedBy: DynamicCodingKey.self)
      var payload = container.nestedContainer(
        keyedBy: ShareCommandCodingKeys.self,
        forKey: DynamicCodingKey("UnmountShare")
      )
      try payload.encode(name, forKey: .name)
    case .fileDropStart(let transferID, let fileName, let sizeBytes):
      var container = encoder.container(keyedBy: DynamicCodingKey.self)
      var payload = container.nestedContainer(
        keyedBy: FileDropStartCodingKeys.self,
        forKey: DynamicCodingKey("FileDropStart")
      )
      try payload.encode(transferID, forKey: .transferID)
      try payload.encode(fileName, forKey: .fileName)
      try payload.encode(sizeBytes, forKey: .sizeBytes)
    case .fileDropChunk(let transferID, let chunkIndex, let dataBase64):
      var container = encoder.container(keyedBy: DynamicCodingKey.self)
      var payload = container.nestedContainer(
        keyedBy: FileDropChunkCodingKeys.self,
        forKey: DynamicCodingKey("FileDropChunk")
      )
      try payload.encode(transferID, forKey: .transferID)
      try payload.encode(chunkIndex, forKey: .chunkIndex)
      try payload.encode(dataBase64, forKey: .dataBase64)
    case .fileDropComplete(let transferID):
      var container = encoder.container(keyedBy: DynamicCodingKey.self)
      var payload = container.nestedContainer(
        keyedBy: FileDropCompleteCodingKeys.self,
        forKey: DynamicCodingKey("FileDropComplete")
      )
      try payload.encode(transferID, forKey: .transferID)
    case .listApplications:
      var container = encoder.singleValueContainer()
      try container.encode("ListApplications")
    case .launchApplication(let id):
      var container = encoder.container(keyedBy: DynamicCodingKey.self)
      var payload = container.nestedContainer(
        keyedBy: IDCommandCodingKeys.self,
        forKey: DynamicCodingKey("LaunchApplication")
      )
      try payload.encode(id, forKey: .id)
    case .listWindows:
      var container = encoder.singleValueContainer()
      try container.encode("ListWindows")
    case .focusWindow(let id):
      var container = encoder.container(keyedBy: DynamicCodingKey.self)
      var payload = container.nestedContainer(
        keyedBy: IDCommandCodingKeys.self,
        forKey: DynamicCodingKey("FocusWindow")
      )
      try payload.encode(id, forKey: .id)
    case .closeWindow(let id):
      var container = encoder.container(keyedBy: DynamicCodingKey.self)
      var payload = container.nestedContainer(
        keyedBy: IDCommandCodingKeys.self,
        forKey: DynamicCodingKey("CloseWindow")
      )
      try payload.encode(id, forKey: .id)
    case .timeSync(let unixEpochMillis):
      var container = encoder.container(keyedBy: DynamicCodingKey.self)
      var payload = container.nestedContainer(
        keyedBy: TimeSyncCodingKeys.self,
        forKey: DynamicCodingKey("TimeSync")
      )
      try payload.encode(unixEpochMillis, forKey: .unixEpochMillis)
    }
  }
}

struct DaemonRunnerStatusRequest: Encodable {
  let type = "runner_status"
  var name: String
}

struct DaemonQemuArgsRequest: Encodable {
  let type = "qemu_args"
  var name: String
}

struct DaemonPrepareRunRequest: Encodable {
  let type = "prepare_run"
  var name: String
}

struct DaemonRunBackendRequest: Encodable {
  let type = "run_backend"
  var name: String
  var spawn: Bool
}

struct DaemonSuspendBackendRequest: Encodable {
  let type = "suspend_backend"
  var name: String
}

struct DaemonResumeBackendRequest: Encodable {
  let type = "resume_backend"
  var name: String
}

struct DaemonLifecyclePlanRequest: Encodable {
  let type = "lifecycle_plan"
  var name: String
  var action: LifecyclePlanAction
}

struct DaemonOpenPortRequest: Encodable {
  let type = "open_port"
  var name: String
  var guest: UInt16
  var scheme: String
}

struct DaemonNetworkPlanRequest: Encodable {
  let type = "plan_network"
  var name: String
}

struct DaemonSSHPlanRequest: Encodable {
  let type = "ssh_plan"
  var name: String
  var user: String?
}

struct DaemonListPortsRequest: Encodable {
  let type = "list_ports"
  var name: String
}

struct DaemonAddPortRequest: Encodable {
  let type = "add_port"
  var name: String
  var host: UInt16
  var guest: UInt16
}

struct DaemonRemovePortRequest: Encodable {
  let type = "remove_port"
  var name: String
  var host: UInt16
  var guest: UInt16
}

struct DaemonListSharesRequest: Encodable {
  let type = "list_shares"
  var name: String
}

struct DaemonAddShareRequest: Encodable {
  let type = "add_share"
  var name: String
  var share: String
  var hostPath: String
  var readOnly: Bool
  var hostPathToken: String?

  enum CodingKeys: String, CodingKey {
    case type
    case name
    case share
    case hostPath = "host_path"
    case readOnly = "read_only"
    case hostPathToken = "host_path_token"
  }
}

struct DaemonRemoveShareRequest: Encodable {
  let type = "remove_share"
  var name: String
  var share: String
}

struct DaemonSnapshotPreflightStatusRequest: Encodable {
  let type = "snapshot_preflight_status"
  var name: String
  var consistency: SnapshotConsistency
}

struct DaemonListSnapshotsRequest: Encodable {
  let type = "list_snapshots"
  var vm: String
}

struct DaemonSnapshotChainRequest: Encodable {
  let type = "snapshot_chain"
  var vm: String
}

struct DaemonCreateSnapshotRequest: Encodable {
  let type = "create_snapshot"
  var vm: String
  var name: String
  var kind: VMSnapshotKind
}

struct DaemonCreateSnapshotDiskRequest: Encodable {
  let type = "create_snapshot_disk"
  var vm: String
  var name: String
}

struct DaemonPrepareDiskRequest: Encodable {
  let type = "prepare_disk"
  var name: String
}

struct DaemonCreateDiskRequest: Encodable {
  let type = "create_disk"
  var name: String
}

struct DaemonInspectDiskRequest: Encodable {
  let type = "inspect_disk"
  var name: String
}

struct DaemonVerifyDiskRequest: Encodable {
  let type = "verify_disk"
  var name: String
}

struct DaemonCompactDiskRequest: Encodable {
  let type = "compact_disk"
  var name: String
}

struct DaemonRepairMetadataRequest: Encodable {
  let type = "repair_metadata"
  var name: String
}

struct DaemonMigrateManifestRequest: Encodable {
  let type = "migrate_manifest"
  var name: String
  var dryRun: Bool

  enum CodingKeys: String, CodingKey {
    case type
    case name
    case dryRun = "dry_run"
  }
}

struct DaemonRestoreSnapshotRequest: Encodable {
  let type = "restore_snapshot"
  var vm: String
  var name: String
}

struct DaemonExecuteApplicationConsistentSnapshotRequest: Encodable {
  let type = "execute_application_consistent_snapshot"
  var vm: String
  var name: String
  var freezeTimeoutMillis: UInt64?

  enum CodingKeys: String, CodingKey {
    case type
    case vm
    case name
    case freezeTimeoutMillis = "freeze_timeout_millis"
  }
}

struct DaemonReapplyRuntimeResourcesRequest: Encodable {
  let type = "reapply_runtime_resources"
  var name: String
  var visibility: RuntimeResourceVisibility
}

struct DaemonCreateDiagnosticBundleRequest: Encodable {
  let type = "create_diagnostic_bundle"
  var name: String
  var output: String?
}

struct DaemonCreatePerformanceBaselineRequest: Encodable {
  let type = "create_performance_baseline"
  var name: String
  var output: String?
}

struct DaemonCreatePerformanceSampleRequest: Encodable {
  let type = "create_performance_sample"
  var name: String
  var output: String?
  var artifactBytes: UInt64
  var iterations: UInt16
  var sync: Bool

  enum CodingKeys: String, CodingKey {
    case type
    case name
    case output
    case artifactBytes = "artifact_bytes"
    case iterations
    case sync
  }
}

struct DaemonStoreDoctorRequest: Encodable {
  let type = "doctor"
}

struct DaemonQMPStatusRequest: Encodable {
  let type = "qmp_status"
  var name: String
}

struct DaemonViewLogsRequest: Encodable {
  let type = "view_logs"
  var name: String
  var kind: VMLogKind
  var maxBytes: UInt64?

  enum CodingKeys: String, CodingKey {
    case type
    case name
    case kind
    case maxBytes = "max_bytes"
  }
}

struct DaemonStopVirtualMachineRequest: Encodable {
  let type = "stop_backend"
  var name: String
}

struct DaemonCreateVirtualMachineRequest: Encodable {
  let type = "create_vm"
  var manifest: DaemonVirtualMachineManifestDTO

  init(createRequest: CreateVirtualMachineRequest) {
    manifest = DaemonVirtualMachineManifestDTO(createRequest: createRequest)
  }
}

struct DaemonRecommendModeRequest: Encodable {
  let type = "recommend_mode"
  var choice: GuestChoice
}

struct DaemonCloneVirtualMachineRequest: Encodable {
  let type = "clone_vm"
  var name: String
  var newName: String
  var linked = false

  enum CodingKeys: String, CodingKey {
    case type
    case name
    case newName = "new_name"
    case linked
  }

  func encode(to encoder: Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    try container.encode(type, forKey: .type)
    try container.encode(name, forKey: .name)
    try container.encode(newName, forKey: .newName)
    if linked {
      try container.encode(linked, forKey: .linked)
    }
  }
}

struct DaemonDeleteVirtualMachineRequest: Encodable {
  let type = "delete_vm"
  var name: String
  var metadataOnly = true

  enum CodingKeys: String, CodingKey {
    case type
    case name
    case metadataOnly = "metadata_only"
  }
}

struct DaemonExportVirtualMachineRequest: Encodable {
  let type = "export_vm"
  var name: String
  var output: String
}

struct DaemonImportVirtualMachineRequest: Encodable {
  let type = "import_vm"
  var input: String
  var name: String?

  enum CodingKeys: String, CodingKey {
    case type
    case input
    case name
  }

  func encode(to encoder: Encoder) throws {
    var container = encoder.container(keyedBy: CodingKeys.self)
    try container.encode(type, forKey: .type)
    try container.encode(input, forKey: .input)
    if let name {
      try container.encode(name, forKey: .name)
    } else {
      try container.encodeNil(forKey: .name)
    }
  }
}

extension DaemonImportBootMediaRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .mediaOperation }
}

extension DaemonVerifyBootMediaRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .mediaOperation }
}

extension DaemonDownloadBootMediaRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .mediaOperation }
}

extension DaemonMountSharedFolderRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .guestToolsCommand }
}

extension DaemonGuestToolsSendCommandRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .guestToolsCommand }
}

extension DaemonPrepareRunRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .lifecycleAction }
}

extension DaemonRunBackendRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .lifecycleAction }
}

extension DaemonSuspendBackendRequest: DaemonRequestTimeoutProviding {
  // Suspend is synchronous: the daemon boots the Fast VM, runs it briefly,
  // pauses, and saves machine state before responding, so it needs a longer
  // budget than a metadata-only lifecycle action.
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .snapshotOperation }
}

extension DaemonResumeBackendRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .lifecycleAction }
}

extension DaemonStopVirtualMachineRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .lifecycleAction }
}

extension DaemonCreateSnapshotRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .snapshotOperation }
}

extension DaemonCreateSnapshotDiskRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .snapshotOperation }
}

extension DaemonRestoreSnapshotRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .snapshotOperation }
}

extension DaemonExecuteApplicationConsistentSnapshotRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .snapshotOperation }
}

extension DaemonPrepareDiskRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .diskOperation }
}

extension DaemonCreateDiskRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .diskOperation }
}

extension DaemonVerifyDiskRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .diskOperation }
}

extension DaemonCompactDiskRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .diskOperation }
}

extension DaemonCreateDiagnosticBundleRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .diagnosticsOperation }
}

extension DaemonCreatePerformanceBaselineRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .diagnosticsOperation }
}

extension DaemonCreatePerformanceSampleRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .diagnosticsOperation }
}

extension DaemonCreateVirtualMachineRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .archiveOperation }
}

extension DaemonCloneVirtualMachineRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .archiveOperation }
}

extension DaemonDeleteVirtualMachineRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .archiveOperation }
}

extension DaemonExportVirtualMachineRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .archiveOperation }
}

extension DaemonImportVirtualMachineRequest: DaemonRequestTimeoutProviding {
  var daemonRequestTimeoutCategory: DaemonRequestTimeoutCategory { .archiveOperation }
}

struct DaemonStateResponse: Decodable {
  var type: String
  var name: String?
}

private struct DaemonErrorResponse: Decodable {
  let type: String
  let message: String
}

struct DaemonRunnerStatusResponse: Decodable {
  var type: String
  var metadata: DaemonRunnerMetadataDTO?
  var qmpSupervisor: DaemonQMPSupervisorDTO?

  enum CodingKeys: String, CodingKey {
    case type
    case metadata
    case qmpSupervisor = "qmp_supervisor"
  }

  var runnerStatus: RunnerStatus? {
    metadata?.runnerStatus(qmpSupervisor: qmpSupervisor?.qmpSupervisor)
  }
}

struct DaemonQemuCommandResponse: Decodable {
  var type: String
  var command: DaemonQemuCommandDTO
}

struct DaemonLifecyclePlanResponse: Decodable {
  var type: String
  var plan: DaemonLifecyclePlanDTO
}

struct DaemonOpenPortResponse: Decodable {
  var type: String
  var plan: DaemonOpenPortDTO
}

struct DaemonNetworkPlanResponse: Decodable {
  var type: String
  var plan: DaemonNetworkPlanDTO
}

struct DaemonSSHPlanResponse: Decodable {
  var type: String
  var plan: DaemonSSHPlanDTO
}

struct DaemonPortForwardsResponse: Decodable {
  var type: String
  var ports: DaemonPortForwardListDTO
}

struct DaemonSharedFoldersResponse: Decodable {
  var type: String
  var shares: DaemonSharedFolderListDTO
}

struct DaemonSnapshotPreflightStatusResponse: Decodable {
  var type: String
  var preflight: DaemonSnapshotPreflightStatusDTO
}

struct DaemonSnapshotListResponse: Decodable {
  var type: String
  var snapshots: [DaemonSnapshotDTO]
}

struct DaemonSnapshotChainResponse: Decodable {
  var type: String
  var chain: DaemonSnapshotChainDTO
}

struct DaemonSnapshotCreatedResponse: Decodable {
  var type: String
  var snapshot: DaemonSnapshotDTO
}

struct DaemonSnapshotDiskCreatedResponse: Decodable {
  var type: String
  var metadata: DaemonSnapshotDiskCreationDTO
}

struct DaemonDiskPreparedResponse: Decodable {
  var type: String
  var metadata: DaemonDiskPreparationDTO
}

struct DaemonDiskCreatedResponse: Decodable {
  var type: String
  var metadata: DaemonDiskCreationDTO
}

struct DaemonDiskInspectedResponse: Decodable {
  var type: String
  var metadata: DaemonDiskInspectionDTO
}

struct DaemonDiskVerifiedResponse: Decodable {
  var type: String
  var metadata: DaemonDiskVerificationDTO
}

struct DaemonDiskCompactedResponse: Decodable {
  var type: String
  var metadata: DaemonDiskCompactionDTO
}

struct DaemonMetadataRepairedResponse: Decodable {
  var type: String
  var repair: DaemonMetadataRepairDTO
}

struct DaemonManifestMigratedResponse: Decodable {
  var type: String
  var migration: DaemonManifestMigrationDTO
}

struct DaemonSnapshotRestoredResponse: Decodable {
  var type: String
  var restore: DaemonSnapshotRestoreDTO
}

struct DaemonApplicationConsistentSnapshotExecutionResponse: Decodable {
  var type: String
  var execution: DaemonApplicationConsistentSnapshotExecutionDTO
}

struct DaemonRuntimeResourcePolicyResponse: Decodable {
  var type: String
  var policy: DaemonRuntimeResourcePolicyDTO
}

struct DaemonDiagnosticBundleResponse: Decodable {
  var type: String
  var bundle: DaemonDiagnosticBundleDTO
}

struct DaemonPerformanceBaselineResponse: Decodable {
  var type: String
  var baseline: DaemonPerformanceBaselineDTO
}

struct DaemonPerformanceSampleResponse: Decodable {
  var type: String
  var sample: DaemonPerformanceSampleDTO
}

struct DaemonQMPStatusResponse: Decodable {
  var type: String
  var status: DaemonQMPStatusDTO
}

struct DaemonLogsViewedResponse: Decodable {
  var type: String
  var log: DaemonVMLogViewDTO
}

struct DaemonExportVirtualMachineResponse: Decodable {
  var type: String
  var export: DaemonExportVirtualMachineMetadataDTO
}

struct DaemonImportVirtualMachineResponse: Decodable {
  var type: String
  var `import`: DaemonImportVirtualMachineMetadataDTO
}

struct DaemonStoreDoctorResponse: Decodable {
  var type: String
  var storeRoot: String
  var vmsDir: String
  var status: String

  enum CodingKeys: String, CodingKey {
    case type
    case storeRoot = "store_root"
    case vmsDir = "vms_dir"
    case status
  }
}

struct DaemonVMLogViewDTO: Decodable {
  var vm: String
  var kind: VMLogKind
  var path: String
  var exists: Bool
  var bytes: UInt64
  var returnedBytes: UInt64
  var truncated: Bool
  var content: String

  enum CodingKeys: String, CodingKey {
    case vm
    case kind
    case path
    case exists
    case bytes
    case returnedBytes = "returned_bytes"
    case truncated
    case content
  }

  var vmLogView: VMLogView {
    VMLogView(
      vm: vm,
      kind: kind,
      path: path,
      exists: exists,
      bytes: bytes,
      returnedBytes: returnedBytes,
      truncated: truncated,
      content: content
    )
  }
}

struct DaemonQMPStatusDTO: Decodable {
  var socketPath: String
  var available: Bool
  var status: String?
  var running: Bool?
  var supervisor: DaemonQMPSupervisorDTO?

  enum CodingKeys: String, CodingKey {
    case socketPath = "socket_path"
    case available
    case status
    case running
    case supervisor
  }

  var qmpStatus: QMPStatus {
    QMPStatus(
      socketPath: socketPath,
      available: available,
      status: status,
      running: running,
      supervisor: supervisor?.qmpSupervisor
    )
  }
}

struct DaemonQMPSupervisorDTO: Decodable {
  var events: [DaemonQMPSupervisorEventDTO]
  var terminalEvent: DaemonQMPSupervisorEventDTO?
  var envelopesRead: Int
  var limitReached: Bool
  var updatedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case events
    case terminalEvent = "terminal_event"
    case envelopesRead = "envelopes_read"
    case limitReached = "limit_reached"
    case updatedAtUnix = "updated_at_unix"
  }

  var qmpSupervisor: QMPSupervisor {
    QMPSupervisor(
      events: events.map(\.qmpSupervisorEvent),
      terminalEvent: terminalEvent?.qmpSupervisorEvent,
      envelopesRead: envelopesRead,
      limitReached: limitReached,
      updatedAtUnix: updatedAtUnix
    )
  }
}

struct DaemonQMPSupervisorEventDTO: Decodable {
  var name: String

  var qmpSupervisorEvent: QMPSupervisorEvent {
    QMPSupervisorEvent(name: name)
  }
}

struct DaemonLifecyclePlanDTO: Decodable {
  var vm: String
  var action: LifecyclePlanAction
  var currentState: String
  var targetState: String
  var backend: String
  var metadataOnly: Bool
  var executable: Bool
  var qmpCommand: String?
  var socketPath: String?
  var socketAvailable: Bool
  var blockers: [String]
  var notes: [String]

  enum CodingKeys: String, CodingKey {
    case vm
    case action
    case currentState = "current_state"
    case targetState = "target_state"
    case backend
    case metadataOnly = "metadata_only"
    case executable
    case qmpCommand = "qmp_command"
    case socketPath = "socket_path"
    case socketAvailable = "socket_available"
    case blockers
    case notes
  }

  var lifecyclePlan: LifecyclePlan {
    LifecyclePlan(
      vm: vm,
      action: action,
      currentState: VirtualMachine.Status(daemonValue: currentState),
      targetState: VirtualMachine.Status(daemonValue: targetState),
      backend: backend,
      metadataOnly: metadataOnly,
      executable: executable,
      qmpCommand: qmpCommand,
      socketPath: socketPath,
      socketAvailable: socketAvailable,
      blockers: blockers,
      notes: notes
    )
  }
}

struct DaemonQemuCommandDTO: Decodable {
  var program: String
  var args: [String]

  var qemuLaunchPlan: QemuLaunchPlan {
    QemuLaunchPlan(program: program, args: args)
  }
}

struct DaemonOpenPortDTO: Decodable {
  var vm: String
  var scheme: String
  var host: String
  var guestPort: UInt16
  var hostPort: UInt16
  var url: String
  var command: [String]

  enum CodingKeys: String, CodingKey {
    case vm
    case scheme
    case host
    case guestPort = "guest_port"
    case hostPort = "host_port"
    case url
    case command
  }

  var openPortPlan: OpenPortPlan {
    OpenPortPlan(
      vm: vm,
      scheme: scheme,
      host: host,
      guestPort: guestPort,
      hostPort: hostPort,
      url: url,
      command: command
    )
  }
}

struct DaemonNetworkPlanDTO: Decodable {
  var vm: String
  var backend: String
  var mode: String
  var hostname: String
  var dryRun: Bool
  var executable: Bool
  var portForwards: [DaemonPortForwardDTO]
  var capabilities: DaemonNetworkCapabilitiesDTO?
  var blockers: [DaemonNetworkPlanBlockerDTO]
  var notes: [String]

  enum CodingKeys: String, CodingKey {
    case vm
    case backend
    case mode
    case hostname
    case dryRun = "dry_run"
    case executable
    case portForwards = "port_forwards"
    case capabilities
    case blockers
    case notes
  }

  var networkPlan: NetworkPlan {
    NetworkPlan(
      vm: vm,
      backend: backend,
      mode: mode,
      hostname: hostname,
      dryRun: dryRun,
      executable: executable,
      portForwards: portForwards.map(\.vmPortForward),
      capabilities: capabilities?.networkCapabilities,
      blockers: blockers.map(\.networkPlanBlocker),
      notes: notes
    )
  }
}

struct DaemonNetworkCapabilitiesDTO: Decodable {
  var guestOutbound: Bool
  var hostToGuest: Bool
  var guestToHost: Bool
  var hostVisibleHostname: Bool
  var supportsPortForwarding: Bool
  var requiresPrivilegedHelper: Bool

  enum CodingKeys: String, CodingKey {
    case guestOutbound = "guest_outbound"
    case hostToGuest = "host_to_guest"
    case guestToHost = "guest_to_host"
    case hostVisibleHostname = "host_visible_hostname"
    case supportsPortForwarding = "supports_port_forwarding"
    case requiresPrivilegedHelper = "requires_privileged_helper"
  }

  var networkCapabilities: NetworkCapabilities {
    NetworkCapabilities(
      guestOutbound: guestOutbound,
      hostToGuest: hostToGuest,
      guestToHost: guestToHost,
      hostVisibleHostname: hostVisibleHostname,
      supportsPortForwarding: supportsPortForwarding,
      requiresPrivilegedHelper: requiresPrivilegedHelper
    )
  }
}

struct DaemonNetworkPlanBlockerDTO: Decodable {
  var code: String
  var message: String

  var networkPlanBlocker: NetworkPlanBlocker {
    NetworkPlanBlocker(code: code, message: message)
  }
}

struct DaemonSSHPlanDTO: Decodable {
  var vm: String
  var user: String
  var host: String
  var port: UInt16
  var source: SSHPlan.Source
  var command: [String]

  var sshPlan: SSHPlan {
    SSHPlan(
      vm: vm,
      user: user,
      host: host,
      port: port,
      source: source,
      command: command
    )
  }
}

struct DaemonPortForwardListDTO: Decodable {
  var vm: String
  var forwards: [DaemonPortForwardDTO]

  var vmPortForwardList: VMPortForwardList {
    VMPortForwardList(
      vm: vm,
      forwards: forwards.map(\.vmPortForward)
    )
  }
}

struct DaemonPortForwardDTO: Decodable {
  var host: UInt16
  var guest: UInt16

  var vmPortForward: VMPortForward {
    VMPortForward(host: host, guest: guest)
  }
}

struct DaemonSharedFolderListDTO: Decodable {
  var vm: String
  var sharedFolders: [DaemonSharedFolderDTO]

  enum CodingKeys: String, CodingKey {
    case vm
    case sharedFolders = "shared_folders"
  }

  var vmSharedFolderList: VMSharedFolderList {
    VMSharedFolderList(
      vm: vm,
      sharedFolders: sharedFolders.map(\.vmSharedFolder)
    )
  }
}

struct DaemonSharedFolderDTO: Decodable {
  var name: String
  var hostPath: String
  var readOnly: Bool
  var hostPathToken: String

  enum CodingKeys: String, CodingKey {
    case name
    case hostPath = "host_path"
    case readOnly = "read_only"
    case hostPathToken = "host_path_token"
  }

  var vmSharedFolder: VMSharedFolder {
    VMSharedFolder(
      name: name,
      hostPath: hostPath,
      readOnly: readOnly,
      hostPathToken: hostPathToken
    )
  }
}

struct DaemonSnapshotDTO: Decodable {
  var name: String
  var kind: VMSnapshotKind
  var createdAtUnix: UInt64
  var vmState: String

  enum CodingKeys: String, CodingKey {
    case name
    case kind
    case createdAtUnix = "created_at_unix"
    case vmState = "vm_state"
  }

  var vmSnapshot: VMSnapshot {
    VMSnapshot(
      name: name,
      kind: kind,
      createdAtUnix: createdAtUnix,
      vmState: VirtualMachine.Status(daemonValue: vmState)
    )
  }
}

struct DaemonSnapshotRestoreDTO: Decodable {
  var snapshot: String
  var restoredAtUnix: UInt64
  var restoredState: String
  var activeDisk: DaemonSnapshotActiveDiskDTO?
  var suspendImage: DaemonSnapshotSuspendImageDTO?

  enum CodingKeys: String, CodingKey {
    case snapshot
    case restoredAtUnix = "restored_at_unix"
    case restoredState = "restored_state"
    case activeDisk = "active_disk"
    case suspendImage = "suspend_image"
  }

  var snapshotRestoreResult: SnapshotRestoreResult {
    SnapshotRestoreResult(
      snapshot: snapshot,
      restoredAtUnix: restoredAtUnix,
      restoredState: VirtualMachine.Status(daemonValue: restoredState),
      activeDisk: activeDisk?.snapshotActiveDisk,
      suspendImage: suspendImage?.snapshotSuspendImage
    )
  }
}

struct DaemonSnapshotActiveDiskDTO: Decodable {
  var source: String
  var snapshot: String?
  var path: String
  var format: String
  var exists: Bool
  var activatedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case source
    case snapshot
    case path
    case format
    case exists
    case activatedAtUnix = "activated_at_unix"
  }

  var snapshotActiveDisk: SnapshotActiveDisk {
    SnapshotActiveDisk(
      source: source,
      snapshot: snapshot,
      path: path,
      format: format,
      exists: exists,
      activatedAtUnix: activatedAtUnix
    )
  }
}

struct DaemonSnapshotSuspendImageDTO: Decodable {
  var snapshot: String
  var imagePath: String
  var imageFormat: String
  var imageExists: Bool
  var preparedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case snapshot
    case imagePath = "image_path"
    case imageFormat = "image_format"
    case imageExists = "image_exists"
    case preparedAtUnix = "prepared_at_unix"
  }

  var snapshotSuspendImage: SnapshotSuspendImage {
    SnapshotSuspendImage(
      snapshot: snapshot,
      imagePath: imagePath,
      imageFormat: imageFormat,
      imageExists: imageExists,
      preparedAtUnix: preparedAtUnix
    )
  }
}

struct DaemonSnapshotChainDTO: Decodable {
  var activeDisk: DaemonActiveDiskDTO
  var disks: [DaemonSnapshotDiskDTO]

  enum CodingKeys: String, CodingKey {
    case activeDisk = "active_disk"
    case disks
  }

  var vmSnapshotChain: VMSnapshotChain {
    VMSnapshotChain(
      activeDisk: activeDisk.vmActiveDisk,
      disks: disks.map(\.vmSnapshotDisk)
    )
  }
}

struct DaemonActiveDiskDTO: Decodable {
  var source: String
  var snapshot: String?
  var path: String
  var format: String
  var exists: Bool
  var activatedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case source
    case snapshot
    case path
    case format
    case exists
    case activatedAtUnix = "activated_at_unix"
  }

  var vmActiveDisk: VMActiveDisk {
    VMActiveDisk(
      source: source,
      snapshot: snapshot,
      path: path,
      format: format,
      exists: exists,
      activatedAtUnix: activatedAtUnix
    )
  }
}

struct DaemonSnapshotDiskDTO: Decodable {
  var snapshot: String
  var overlayPath: String
  var overlayFormat: String
  var overlayExists: Bool
  var backingPath: String
  var backingFormat: String
  var backingExists: Bool
  var createCommand: [String]
  var preparedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case snapshot
    case overlayPath = "overlay_path"
    case overlayFormat = "overlay_format"
    case overlayExists = "overlay_exists"
    case backingPath = "backing_path"
    case backingFormat = "backing_format"
    case backingExists = "backing_exists"
    case createCommand = "create_command"
    case preparedAtUnix = "prepared_at_unix"
  }

  var vmSnapshotDisk: VMSnapshotDisk {
    VMSnapshotDisk(
      snapshot: snapshot,
      overlayPath: overlayPath,
      overlayFormat: overlayFormat,
      overlayExists: overlayExists,
      backingPath: backingPath,
      backingFormat: backingFormat,
      backingExists: backingExists,
      createCommand: createCommand,
      preparedAtUnix: preparedAtUnix
    )
  }
}

struct DaemonSnapshotDiskCreationDTO: Decodable {
  var snapshot: String
  var disk: DaemonSnapshotDiskDTO
  var command: [String]
  var executed: Bool
  var exitStatus: String?
  var stdout: String
  var stderr: String
  var createdAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case snapshot
    case disk
    case command
    case executed
    case exitStatus = "exit_status"
    case stdout
    case stderr
    case createdAtUnix = "created_at_unix"
  }

  var vmSnapshotDiskCreation: VMSnapshotDiskCreation {
    VMSnapshotDiskCreation(
      snapshot: snapshot,
      disk: disk.vmSnapshotDisk,
      command: command,
      executed: executed,
      exitStatus: exitStatus,
      stdout: stdout,
      stderr: stderr,
      createdAtUnix: createdAtUnix
    )
  }
}

struct DaemonDiskPreparationDTO: Decodable {
  var path: String
  var format: String
  var size: String
  var sizeBytes: UInt64?
  var exists: Bool
  var created: Bool
  var createCommand: [String]?
  var preparedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case path
    case format
    case size
    case sizeBytes = "size_bytes"
    case exists
    case created
    case createCommand = "create_command"
    case preparedAtUnix = "prepared_at_unix"
  }

  var diskPreparation: DiskPreparation {
    DiskPreparation(
      path: path,
      format: format,
      size: size,
      sizeBytes: sizeBytes,
      exists: exists,
      created: created,
      createCommand: createCommand,
      preparedAtUnix: preparedAtUnix
    )
  }
}

struct DaemonDiskCreationDTO: Decodable {
  var preparation: DaemonDiskPreparationDTO
  var command: [String]?
  var executed: Bool
  var exitStatus: String?
  var stdout: String
  var stderr: String
  var createdAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case preparation
    case command
    case executed
    case exitStatus = "exit_status"
    case stdout
    case stderr
    case createdAtUnix = "created_at_unix"
  }

  var vmDiskCreation: VMDiskCreation {
    VMDiskCreation(
      preparation: preparation.diskPreparation,
      command: command,
      executed: executed,
      exitStatus: exitStatus,
      stdout: stdout,
      stderr: stderr,
      createdAtUnix: createdAtUnix
    )
  }
}

struct DaemonDiskInspectionDTO: Decodable {
  var preparation: DaemonDiskPreparationDTO
  var command: [String]
  var exitStatus: String
  var info: DiskMetadataValue
  var stdout: String
  var stderr: String
  var inspectDurationMicroseconds: UInt64
  var inspectedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case preparation
    case command
    case exitStatus = "exit_status"
    case info
    case stdout
    case stderr
    case inspectDurationMicroseconds = "inspect_duration_microseconds"
    case inspectedAtUnix = "inspected_at_unix"
  }

  var vmDiskInspection: VMDiskInspection {
    VMDiskInspection(
      preparation: preparation.diskPreparation,
      command: command,
      exitStatus: exitStatus,
      info: info.prettyPrinted,
      infoValue: info,
      stdout: stdout,
      stderr: stderr,
      inspectDurationMicroseconds: inspectDurationMicroseconds,
      inspectedAtUnix: inspectedAtUnix
    )
  }
}

struct DaemonDiskVerificationDTO: Decodable {
  var activeDisk: DaemonActiveDiskDTO
  var command: [String]
  var exitStatus: String
  var report: DiskMetadataValue
  var stdout: String
  var stderr: String
  var verifyDurationMicroseconds: UInt64
  var verifiedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case activeDisk = "active_disk"
    case command
    case exitStatus = "exit_status"
    case report
    case stdout
    case stderr
    case verifyDurationMicroseconds = "verify_duration_microseconds"
    case verifiedAtUnix = "verified_at_unix"
  }

  var vmDiskVerification: VMDiskVerification {
    VMDiskVerification(
      activeDisk: activeDisk.vmActiveDisk,
      command: command,
      exitStatus: exitStatus,
      report: report.prettyPrinted,
      reportValue: report,
      stdout: stdout,
      stderr: stderr,
      verifyDurationMicroseconds: verifyDurationMicroseconds,
      verifiedAtUnix: verifiedAtUnix
    )
  }
}

struct DaemonDiskCompactionDTO: Decodable {
  var preparation: DaemonDiskPreparationDTO
  var activeDisk: DaemonActiveDiskDTO
  var command: [String]
  var tempPath: String
  var backupPath: String
  var exitStatus: String
  var stdout: String
  var stderr: String
  var originalSizeBytes: UInt64
  var compactedSizeBytes: UInt64
  var compactDurationMicroseconds: UInt64
  var compactedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case preparation
    case activeDisk = "active_disk"
    case command
    case tempPath = "temp_path"
    case backupPath = "backup_path"
    case exitStatus = "exit_status"
    case stdout
    case stderr
    case originalSizeBytes = "original_size_bytes"
    case compactedSizeBytes = "compacted_size_bytes"
    case compactDurationMicroseconds = "compact_duration_microseconds"
    case compactedAtUnix = "compacted_at_unix"
  }

  var vmDiskCompaction: VMDiskCompaction {
    VMDiskCompaction(
      preparation: preparation.diskPreparation,
      activeDisk: activeDisk.vmActiveDisk,
      command: command,
      tempPath: tempPath,
      backupPath: backupPath,
      exitStatus: exitStatus,
      stdout: stdout,
      stderr: stderr,
      originalSizeBytes: originalSizeBytes,
      compactedSizeBytes: compactedSizeBytes,
      compactDurationMicroseconds: compactDurationMicroseconds,
      compactedAtUnix: compactedAtUnix
    )
  }
}

struct DaemonMetadataRepairDTO: Decodable {
  var vm: String
  var bundle: String
  var repaired: Bool
  var actions: [DaemonMetadataRepairActionDTO]
  var repairedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case vm
    case bundle
    case repaired
    case actions
    case repairedAtUnix = "repaired_at_unix"
  }

  var vmMetadataRepair: VMMetadataRepair {
    VMMetadataRepair(
      vm: vm,
      bundle: bundle,
      repaired: repaired,
      actions: actions.map(\.vmMetadataRepairAction),
      repairedAtUnix: repairedAtUnix
    )
  }
}

struct DaemonMetadataRepairActionDTO: Decodable {
  var action: String
  var path: String
  var detail: String

  var vmMetadataRepairAction: VMMetadataRepairAction {
    VMMetadataRepairAction(
      action: action,
      path: path,
      detail: detail
    )
  }
}

struct DaemonManifestMigrationDTO: Decodable {
  var vm: String
  var bundle: String
  var manifestPath: String
  var dryRun: Bool
  var migrated: Bool
  var fromSchema: String
  var toSchema: String
  var actions: [String]
  var backupPath: String?
  var receiptPath: String?
  var migratedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case vm
    case bundle
    case manifestPath = "manifest_path"
    case dryRun = "dry_run"
    case migrated
    case fromSchema = "from_schema"
    case toSchema = "to_schema"
    case actions
    case backupPath = "backup_path"
    case receiptPath = "receipt_path"
    case migratedAtUnix = "migrated_at_unix"
  }

  var vmManifestMigration: VMManifestMigration {
    VMManifestMigration(
      vm: vm,
      bundle: bundle,
      manifestPath: manifestPath,
      dryRun: dryRun,
      migrated: migrated,
      fromSchema: fromSchema,
      toSchema: toSchema,
      actions: actions,
      backupPath: backupPath,
      receiptPath: receiptPath,
      migratedAtUnix: migratedAtUnix
    )
  }
}

struct DaemonDiagnosticBundleDTO: Decodable {
  var vm: String
  var source: String
  var output: String
  var files: [String]
  var createdAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case vm
    case source
    case output
    case files
    case createdAtUnix = "created_at_unix"
  }

  var diagnosticBundle: DiagnosticBundle {
    DiagnosticBundle(
      vm: vm,
      source: source,
      output: output,
      files: files,
      createdAtUnix: createdAtUnix
    )
  }
}

struct DaemonPerformanceRuntimeStateDTO: Decodable {
  var state: String

  var virtualMachineStatus: VirtualMachine.Status {
    VirtualMachine.Status(daemonValue: state)
  }
}

struct DaemonPerformanceMeasurementDTO: Decodable {
  var name: String
  var value: UInt64
  var unit: String
  var source: String
  var metadataOnly: Bool

  enum CodingKeys: String, CodingKey {
    case name
    case value
    case unit
    case source
    case metadataOnly = "metadata_only"
  }

  var performanceMeasurement: PerformanceMeasurement {
    PerformanceMeasurement(
      name: name,
      value: value,
      unit: unit,
      source: source,
      metadataOnly: metadataOnly
    )
  }
}

struct DaemonPerformanceBaselineDTO: Decodable {
  var vm: String
  var source: String
  var output: String
  var artifact: String
  var createdAtUnix: UInt64
  var metadataOnly: Bool
  var state: DaemonPerformanceRuntimeStateDTO
  var runner: DaemonRunnerMetadataDTO?
  var guestTools: DaemonGuestToolsStatusDTO
  var metrics: DaemonGuestToolsMetricsDTO?
  var measurements: [DaemonPerformanceMeasurementDTO]
  var notes: [String]

  enum CodingKeys: String, CodingKey {
    case vm
    case source
    case output
    case artifact
    case createdAtUnix = "created_at_unix"
    case metadataOnly = "metadata_only"
    case state
    case runner
    case guestTools = "guest_tools"
    case metrics
    case measurements
    case notes
  }

  var performanceBaseline: PerformanceBaseline {
    PerformanceBaseline(
      vm: vm,
      source: source,
      output: output,
      artifact: artifact,
      createdAtUnix: createdAtUnix,
      metadataOnly: metadataOnly,
      state: state.virtualMachineStatus,
      runner: runner?.runnerStatus(),
      guestTools: guestTools.guestToolsStatus,
      metrics: metrics?.guestToolsMetrics,
      measurements: measurements.map(\.performanceMeasurement),
      notes: notes
    )
  }
}

struct DaemonPerformanceSampleDTO: Decodable {
  var vm: String
  var source: String
  var output: String
  var artifact: String
  var probe: String
  var probes: [String]
  var artifactBytes: UInt64
  var iterations: UInt16
  var sync: Bool
  var iterationResults: [DaemonPerformanceSampleIterationDTO]
  var createdAtUnix: UInt64
  var state: DaemonPerformanceRuntimeStateDTO
  var runner: DaemonRunnerMetadataDTO?
  var guestTools: DaemonGuestToolsStatusDTO
  var metrics: DaemonGuestToolsMetricsDTO?
  var measurements: [DaemonPerformanceMeasurementDTO]
  var notes: [String]

  enum CodingKeys: String, CodingKey {
    case vm
    case source
    case output
    case artifact
    case probe
    case probes
    case artifactBytes = "artifact_bytes"
    case iterations
    case sync
    case iterationResults = "iteration_results"
    case createdAtUnix = "created_at_unix"
    case state
    case runner
    case guestTools = "guest_tools"
    case metrics
    case measurements
    case notes
  }

  var performanceSample: PerformanceSample {
    PerformanceSample(
      vm: vm,
      source: source,
      output: output,
      artifact: artifact,
      probe: probe,
      probes: probes,
      artifactBytes: artifactBytes,
      iterations: iterations,
      sync: sync,
      iterationResults: iterationResults.map(\.performanceSampleIteration),
      createdAtUnix: createdAtUnix,
      state: state.virtualMachineStatus,
      runner: runner?.runnerStatus(),
      guestTools: guestTools.guestToolsStatus,
      metrics: metrics?.guestToolsMetrics,
      measurements: measurements.map(\.performanceMeasurement),
      notes: notes
    )
  }
}

struct DaemonPerformanceSampleIterationDTO: Decodable {
  var iteration: UInt16
  var probe: String
  var bytes: UInt64
  var writeLatencyMicroseconds: UInt64
  var sync: Bool

  enum CodingKeys: String, CodingKey {
    case iteration
    case probe
    case bytes
    case writeLatencyMicroseconds = "write_latency_microseconds"
    case sync
  }

  var performanceSampleIteration: PerformanceSampleIteration {
    PerformanceSampleIteration(
      iteration: iteration,
      probe: probe,
      bytes: bytes,
      writeLatencyMicroseconds: writeLatencyMicroseconds,
      sync: sync
    )
  }
}

struct DaemonListVirtualMachinesResponse: Decodable {
  var virtualMachines: [DaemonVirtualMachineDTO]

  enum CodingKeys: String, CodingKey {
    case virtualMachines
    case virtualMachinesSnake = "virtual_machines"
    case result
    case vms
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)

    if let virtualMachines = try container.decodeIfPresent(
      [DaemonVirtualMachineDTO].self, forKey: .virtualMachines)
    {
      self.virtualMachines = virtualMachines
    } else if let virtualMachines = try container.decodeIfPresent(
      [DaemonVirtualMachineDTO].self, forKey: .virtualMachinesSnake)
    {
      self.virtualMachines = virtualMachines
    } else if let virtualMachines = try container.decodeIfPresent(
      [DaemonVirtualMachineDTO].self, forKey: .vms)
    {
      self.virtualMachines = virtualMachines
    } else if let result = try container.decodeIfPresent(
      DaemonListVirtualMachinesResult.self, forKey: .result)
    {
      self.virtualMachines = result.virtualMachines
    } else {
      throw VirtualMachineClientError.daemonResponseInvalid
    }
  }
}

struct DaemonListBootTemplatesResponse: Decodable {
  var templates: [DaemonBootTemplateDTO]
}

struct DaemonModeRecommendationResponse: Decodable {
  var recommendation: ModeRecommendation
}

struct DaemonBootMediaStatusResponse: Decodable {
  var type: String
  var status: DaemonBootMediaStatusDTO
}

struct DaemonReadinessReportResponse: Decodable {
  var type: String
  var report: DaemonReadinessReportDTO
}

struct DaemonBootMediaImportResponse: Decodable {
  var type: String
  var `import`: DaemonBootMediaImportMetadataDTO
}

struct DaemonBootMediaVerificationResponse: Decodable {
  var type: String
  var verification: DaemonBootMediaVerificationMetadataDTO
}

struct DaemonBootMediaDownloadPlanResponse: Decodable {
  var type: String
  var plan: DaemonBootMediaDownloadPlanMetadataDTO
}

struct DaemonBootMediaDownloadResponse: Decodable {
  var type: String
  var download: DaemonBootMediaDownloadResultMetadataDTO
}

struct DaemonGuestToolsStatusResponse: Decodable {
  var type: String
  var status: DaemonGuestToolsStatusDTO
}

struct DaemonGuestToolsTokenResponse: Decodable {
  var type: String
  var token: DaemonGuestToolsTokenDTO
}

struct DaemonGuestToolsLinuxCommandResponse: Decodable {
  var type: String
  var command: DaemonGuestToolsLinuxCommandDTO
}

struct DaemonMountSharedFolderResponse: Decodable {
  var type: String
  var command: DaemonGuestToolsCommandDTO?
}

struct DaemonGuestToolsCommandResponse: Decodable {
  var type: String
  var command: DaemonGuestToolsCommandDTO?
}

struct DaemonGuestToolsCommandDTO: Decodable {
  var vm: String
  var requestID: String?
  var pendingCommands: Int

  enum CodingKeys: String, CodingKey {
    case vm
    case requestID = "request_id"
    case pendingCommands = "pending_commands"
  }

  var guestToolsCommandDispatch: GuestToolsCommandDispatch {
    GuestToolsCommandDispatch(
      vm: vm,
      requestID: requestID,
      pendingCommands: pendingCommands
    )
  }
}

struct DaemonGuestToolsTokenDTO: Decodable {
  var vm: String
  private var token: String
  var createdAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case vm
    case token
    case createdAtUnix = "created_at_unix"
  }

  var guestToolsToken: GuestToolsToken {
    GuestToolsToken(vm: vm, createdAtUnix: createdAtUnix, tokenLength: token.count)
  }
}

struct DaemonGuestToolsLinuxCommandDTO: Decodable {
  var vm: String
  var transport: GuestToolsLinuxCommandTransport
  var command: [String]
  var tokenFile: String
  var capabilities: [String]

  enum CodingKeys: String, CodingKey {
    case vm
    case transport
    case command
    case tokenFile = "token_file"
    case capabilities
  }

  var guestToolsLinuxCommand: GuestToolsLinuxCommand {
    GuestToolsLinuxCommand(
      vm: vm,
      transport: transport,
      command: command,
      tokenFile: tokenFile,
      capabilities: capabilities
    )
  }
}

struct DaemonVirtualMachineResponse: Decodable {
  var virtualMachine: DaemonVirtualMachineDTO

  enum CodingKeys: String, CodingKey {
    case virtualMachine
    case vm
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    virtualMachine =
      try container.decodeIfPresent(DaemonVirtualMachineDTO.self, forKey: .virtualMachine)
      ?? container.decode(DaemonVirtualMachineDTO.self, forKey: .vm)
  }
}

struct DaemonCloneVirtualMachineResponse: Decodable {
  var type: String
  var clone: DaemonCloneVirtualMachineMetadataDTO
}

struct DaemonDeleteVirtualMachineResponse: Decodable {
  var type: String
  var path: String
  var metadataOnly: Bool
  var metadata: DaemonDeletionMetadataDTO?

  enum CodingKeys: String, CodingKey {
    case type
    case path
    case metadataOnly = "metadata_only"
    case metadata
  }

  var vmDeletionMetadata: VMDeletionMetadata {
    VMDeletionMetadata(
      path: path,
      metadataOnly: metadataOnly,
      vm: metadata?.vm
    )
  }
}

private struct DaemonListVirtualMachinesResult: Decodable {
  var virtualMachines: [DaemonVirtualMachineDTO]

  enum CodingKeys: String, CodingKey {
    case virtualMachines
    case virtualMachinesSnake = "virtual_machines"
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    virtualMachines =
      try container.decodeIfPresent([DaemonVirtualMachineDTO].self, forKey: .virtualMachines)
      ?? container.decode([DaemonVirtualMachineDTO].self, forKey: .virtualMachinesSnake)
  }
}

struct BootTemplate: Identifiable, Codable, Equatable {
  enum BootMode: String, Codable, Equatable {
    case existingDisk = "existing-disk"
    case linuxKernel = "linux-kernel"
    case linuxInstaller = "linux-installer"
    case macosRestore = "macos-restore"

    var title: String {
      switch self {
      case .existingDisk:
        return "Existing disk"
      case .linuxKernel:
        return "Linux kernel"
      case .linuxInstaller:
        return "Linux installer"
      case .macosRestore:
        return "macOS restore"
      }
    }
  }

  var id: String
  var guestOS: String
  var guestVersion: String?
  var guestArch: String
  var mode: BootMode
  var mediaLabel: String
  var source: String
  var installerImage: String?
  var kernelPath: String?
  var initrdPath: String?
  var kernelCommandLine: String?
  var macosRestoreImage: String?
  var note: String

  enum CodingKeys: String, CodingKey {
    case id
    case guestOS = "guest_os"
    case guestVersion = "guest_version"
    case guestArch = "guest_arch"
    case mode
    case mediaLabel = "media_label"
    case source
    case installerImage = "installer_image"
    case kernelPath = "kernel_path"
    case initrdPath = "initrd_path"
    case kernelCommandLine = "kernel_command_line"
    case macosRestoreImage = "macos_restore_image"
    case note
  }

  var guestTitle: String {
    [guestOS.capitalized, guestVersion, guestArch.uppercased()]
      .compactMap { value in
        guard let value, !value.isEmpty else {
          return nil
        }
        return value
      }
      .joined(separator: " ")
  }

  var engineMode: VirtualMachine.EngineMode {
    switch mode {
    case .existingDisk:
      return .compatibility
    case .linuxKernel, .linuxInstaller, .macosRestore:
      return .fast
    }
  }
}

struct CreateVirtualMachineRequest: Equatable {
  var name: String
  var template: BootTemplate
  var diskSize: String = "80GiB"
}

struct GuestChoice: Codable, Equatable {
  var os: String
  var version: String?
  var arch: String

  init(os: String, version: String?, arch: String) {
    self.os = os
    self.version = version
    self.arch = arch
  }

  init(template: BootTemplate) {
    self.init(os: template.guestOS, version: template.guestVersion, arch: template.guestArch)
  }
}

struct ModeRecommendation: Codable, Equatable {
  var mode: VirtualMachine.EngineMode
  var performance: String
  var batteryImpact: String
  var integration: String
  var message: String
  var fastModeAvailable: Bool
  var bootTemplate: BootTemplate?

  enum CodingKeys: String, CodingKey {
    case mode
    case performance
    case batteryImpact = "battery_impact"
    case integration
    case message
    case fastModeAvailable = "fast_mode_available"
    case bootTemplate = "boot_template"
  }
}

struct CloneVirtualMachineMetadata: Equatable {
  var vm: String
  var source: String
  var output: String
  var linked: Bool = false
  var backingPath: String?
  var backingFormat: String?
  var createCommand: [String]?
  var clonedAtUnix: UInt64?

  var createCommandLine: String? {
    createCommand?.joined(separator: " ")
  }
}

struct VMDeletionMetadata: Equatable {
  var path: String
  var metadataOnly: Bool
  var vm: String?
}

struct DaemonBootTemplateDTO: Decodable {
  var id: String
  var guestOS: String
  var guestVersion: String?
  var guestArch: String
  var mode: String
  var mediaLabel: String
  var source: String
  var installerImage: String?
  var kernelPath: String?
  var initrdPath: String?
  var kernelCommandLine: String?
  var macosRestoreImage: String?
  var note: String

  enum CodingKeys: String, CodingKey {
    case id
    case guestOS = "guest_os"
    case guestVersion = "guest_version"
    case guestArch = "guest_arch"
    case mode
    case mediaLabel = "media_label"
    case source
    case installerImage = "installer_image"
    case kernelPath = "kernel_path"
    case initrdPath = "initrd_path"
    case kernelCommandLine = "kernel_command_line"
    case macosRestoreImage = "macos_restore_image"
    case note
  }

  var bootTemplate: BootTemplate {
    BootTemplate(
      id: id,
      guestOS: guestOS,
      guestVersion: guestVersion,
      guestArch: guestArch,
      mode: BootTemplate.BootMode(rawValue: mode) ?? .existingDisk,
      mediaLabel: mediaLabel,
      source: source,
      installerImage: installerImage,
      kernelPath: kernelPath,
      initrdPath: initrdPath,
      kernelCommandLine: kernelCommandLine,
      macosRestoreImage: macosRestoreImage,
      note: note
    )
  }
}

struct DaemonVirtualMachineManifestDTO: Encodable {
  let schemaVersion = "bridgevm.io/v1"
  var name: String
  var mode: VirtualMachine.EngineMode
  var guest: Guest
  var backend: Backend
  var resources: Resources
  var display: Display
  var storage: Storage
  var boot: Boot
  var network: Network
  var integration: Integration
  var security: Security

  init(createRequest: CreateVirtualMachineRequest) {
    let fast = createRequest.template.engineMode == .fast
    name = createRequest.name.trimmingCharacters(in: .whitespacesAndNewlines)
    mode = createRequest.template.engineMode
    guest = Guest(
      os: createRequest.template.guestOS,
      version: createRequest.template.guestVersion,
      arch: createRequest.template.guestArch
    )
    backend =
      fast
      ? Backend(
        engine: "lightvm", preferred: "apple-vz", fallback: "qemu-hvf-restricted",
        accelerator: "hvf")
      : Backend(engine: "qemu", preferred: nil, fallback: "tcg", accelerator: "hvf")
    resources = Resources(profile: "automatic", memory: "auto", cpu: "auto")
    display = Display(renderer: fast ? "metal" : "spice", framePolicy: "adaptive", retina: true)
    storage = Storage(
      primary: PrimaryDisk(
        path: "disks/root.qcow2",
        size: createRequest.diskSize,
        format: "qcow2",
        discard: fast
      )
    )
    boot = Boot(template: createRequest.template)
    network = Network(mode: "nat", hostname: "\(Self.slug(name)).bridgevm.local", forwards: [])
    integration = Integration(
      tools: fast ? "required" : "optional",
      clipboard: true,
      dragDrop: fast,
      dynamicResolution: true,
      sharedFolders: true,
      applications: true,
      windows: true
    )
    security = Security(
      sharedFolderApproval: "required",
      guestCommandExecution: false,
      signedAgentUpdates: true
    )
  }

  private static func slug(_ value: String) -> String {
    var result = ""
    var previousWasSeparator = false

    for scalar in value.unicodeScalars {
      if CharacterSet.alphanumerics.contains(scalar), scalar.isASCII {
        let codepoint = scalar.value + ((65...90).contains(scalar.value) ? 32 : 0)
        result.unicodeScalars.append(UnicodeScalar(codepoint)!)
        previousWasSeparator = false
      } else if !previousWasSeparator, !result.isEmpty {
        result.append("-")
        previousWasSeparator = true
      }
    }

    if result.last == "-" {
      result.removeLast()
    }
    return result.isEmpty ? "vm" : result
  }

  struct Guest: Encodable {
    var os: String
    var version: String?
    var arch: String
  }

  struct Backend: Encodable {
    var engine: String
    var preferred: String?
    var fallback: String?
    var accelerator: String?
  }

  struct Resources: Encodable {
    var profile: String
    var memory: String
    var cpu: String
  }

  struct Display: Encodable {
    var renderer: String
    var framePolicy: String
    var retina: Bool
  }

  struct Storage: Encodable {
    var primary: PrimaryDisk
  }

  struct PrimaryDisk: Encodable {
    var path: String
    var size: String
    var format: String
    var discard: Bool
  }

  struct Boot: Encodable {
    var mode: BootTemplate.BootMode
    var installerImage: String?
    var kernelPath: String?
    var initrdPath: String?
    var kernelCommandLine: String?
    var macosRestoreImage: String?

    init(template: BootTemplate) {
      mode = template.mode
      installerImage = template.installerImage
      kernelPath = template.kernelPath
      initrdPath = template.initrdPath
      kernelCommandLine = template.kernelCommandLine
      macosRestoreImage = template.macosRestoreImage
    }
  }

  struct Network: Encodable {
    var mode: String
    var hostname: String
    var forwards: [PortForward]
  }

  struct PortForward: Encodable {
    var host: UInt16
    var guest: UInt16
  }

  struct Integration: Encodable {
    var tools: String
    var clipboard: Bool
    var dragDrop: Bool
    var dynamicResolution: Bool
    var sharedFolders: Bool
    var applications: Bool
    var windows: Bool
  }

  struct Security: Encodable {
    var sharedFolderApproval: String
    var guestCommandExecution: Bool
    var signedAgentUpdates: Bool
  }
}

struct DaemonVirtualMachineDTO: Decodable {
  var id: UUID?
  var name: String
  var guest: String?
  var guestArch: String?
  var status: String?
  var mode: String?
  var resources: DaemonVirtualMachineResourcesDTO?
  var uptime: String?
  var ipAddress: String?
  var lastStarted: Date?
  var notes: String?

  enum CodingKeys: String, CodingKey {
    case id
    case name
    case guest
    case guestOS = "guest_os"
    case guestArch
    case guestArchSnake = "guest_arch"
    case status
    case state
    case mode
    case engineMode = "engine_mode"
    case resources
    case uptime
    case ipAddress
    case ipAddressSnake = "ip_address"
    case lastStarted
    case lastStartedSnake = "last_started"
    case notes
    case path
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    id = try container.decodeIfPresent(UUID.self, forKey: .id)
    name = try container.decode(String.self, forKey: .name)
    guest =
      try container.decodeIfPresent(String.self, forKey: .guest)
      ?? container.decodeIfPresent(String.self, forKey: .guestOS)
    guestArch =
      try container.decodeIfPresent(String.self, forKey: .guestArch)
      ?? container.decodeIfPresent(String.self, forKey: .guestArchSnake)
    status =
      try container.decodeIfPresent(String.self, forKey: .status)
      ?? container.decodeIfPresent(String.self, forKey: .state)
    mode =
      try container.decodeIfPresent(String.self, forKey: .mode)
      ?? container.decodeIfPresent(String.self, forKey: .engineMode)
    resources = try container.decodeIfPresent(
      DaemonVirtualMachineResourcesDTO.self, forKey: .resources)
    uptime = try container.decodeIfPresent(String.self, forKey: .uptime)
    ipAddress =
      try container.decodeIfPresent(String.self, forKey: .ipAddress)
      ?? container.decodeIfPresent(String.self, forKey: .ipAddressSnake)
    lastStarted = try container.decodeFlexibleDate(keys: [.lastStarted, .lastStartedSnake])
    notes = try container.decodeIfPresent(String.self, forKey: .notes)
  }

  var virtualMachine: VirtualMachine {
    VirtualMachine(
      id: id ?? UUID.bridgeVMStableNameID(name),
      name: name,
      guest: guestTitle.isEmpty ? "Unknown Guest" : guestTitle,
      status: VirtualMachine.Status(daemonValue: status),
      mode: VirtualMachine.EngineMode(daemonValue: mode),
      resources: resources?.resources ?? .init(cpuCount: 0, memoryGB: 0, diskGB: 0),
      uptime: uptime ?? "Unknown",
      ipAddress: ipAddress,
      lastStarted: lastStarted,
      notes: notes ?? ""
    )
  }

  private var guestTitle: String {
    [guest, guestArch]
      .compactMap { value in
        guard let value, !value.isEmpty else {
          return nil
        }
        return value
      }
      .joined(separator: " ")
  }
}

struct DaemonCloneVirtualMachineMetadataDTO: Decodable {
  var vm: String
  var source: String
  var output: String
  var linked: Bool?
  var backingPath: String?
  var backingFormat: String?
  var createCommand: [String]?
  var clonedAtUnix: UInt64?

  enum CodingKeys: String, CodingKey {
    case vm
    case source
    case output
    case linked
    case backingPath = "backing_path"
    case backingFormat = "backing_format"
    case createCommand = "create_command"
    case clonedAtUnix = "cloned_at_unix"
  }

  var cloneVirtualMachineMetadata: CloneVirtualMachineMetadata {
    CloneVirtualMachineMetadata(
      vm: vm,
      source: source,
      output: output,
      linked: linked ?? false,
      backingPath: backingPath,
      backingFormat: backingFormat,
      createCommand: createCommand,
      clonedAtUnix: clonedAtUnix
    )
  }
}

struct DaemonDeletionMetadataDTO: Decodable {
  var vm: String?
}

struct DaemonExportVirtualMachineMetadataDTO: Decodable {
  var vm: String
  var source: String
  var output: String
  var archiveFormat: String
  var copiedFileCount: UInt64
  var copiedFiles: [String]
  var manifestPreserved: Bool
  var metadataPreserved: Bool
  var exportedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case vm
    case source
    case output
    case archiveFormat = "archive_format"
    case copiedFileCount = "copied_file_count"
    case copiedFiles = "copied_files"
    case manifestPreserved = "manifest_preserved"
    case metadataPreserved = "metadata_preserved"
    case exportedAtUnix = "exported_at_unix"
  }

  var vmExportMetadata: VMExportMetadata {
    VMExportMetadata(
      vm: vm,
      source: source,
      output: output,
      archiveFormat: archiveFormat,
      copiedFileCount: copiedFileCount,
      copiedFiles: copiedFiles,
      manifestPreserved: manifestPreserved,
      metadataPreserved: metadataPreserved,
      exportedAtUnix: exportedAtUnix
    )
  }
}

struct DaemonImportVirtualMachineMetadataDTO: Decodable {
  var vm: String
  var source: String
  var output: String
  var archiveFormat: String
  var copiedFileCount: UInt64
  var copiedFiles: [String]
  var manifestPreserved: Bool
  var metadataPreserved: Bool
  var originalName: String
  var requestedName: String?
  var manifestIdentityRewritten: Bool
  var importedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case vm
    case source
    case output
    case archiveFormat = "archive_format"
    case copiedFileCount = "copied_file_count"
    case copiedFiles = "copied_files"
    case manifestPreserved = "manifest_preserved"
    case metadataPreserved = "metadata_preserved"
    case originalName = "original_name"
    case requestedName = "requested_name"
    case manifestIdentityRewritten = "manifest_identity_rewritten"
    case importedAtUnix = "imported_at_unix"
  }

  var vmImportMetadata: VMImportMetadata {
    VMImportMetadata(
      vm: vm,
      source: source,
      output: output,
      archiveFormat: archiveFormat,
      copiedFileCount: copiedFileCount,
      copiedFiles: copiedFiles,
      manifestPreserved: manifestPreserved,
      metadataPreserved: metadataPreserved,
      originalName: originalName,
      requestedName: requestedName,
      manifestIdentityRewritten: manifestIdentityRewritten,
      importedAtUnix: importedAtUnix
    )
  }
}

extension UUID {
  fileprivate static func bridgeVMStableNameID(_ name: String) -> UUID {
    let bytes = Array(name.utf8)
    var hash: UInt64 = 0xcbf2_9ce4_8422_2325
    for byte in bytes {
      hash ^= UInt64(byte)
      hash &*= 0x100_0000_01b3
    }

    var uuid = uuid_t(
      UInt8((hash >> 56) & 0xff),
      UInt8((hash >> 48) & 0xff),
      UInt8((hash >> 40) & 0xff),
      UInt8((hash >> 32) & 0xff),
      UInt8((hash >> 24) & 0xff),
      UInt8((hash >> 16) & 0xff),
      UInt8((hash >> 8) & 0xff),
      UInt8(hash & 0xff),
      UInt8((bytes.count >> 8) & 0xff),
      UInt8(bytes.count & 0xff),
      0x42,
      0x56,
      0x4d,
      0x61,
      0x70,
      0x70
    )
    uuid.6 = (uuid.6 & 0x0f) | 0x40
    uuid.8 = (uuid.8 & 0x3f) | 0x80
    return UUID(uuid: uuid)
  }
}

struct DaemonVirtualMachineResourcesDTO: Decodable {
  var cpuCount: Int?
  var memoryGB: Int?
  var diskGB: Int?

  enum CodingKeys: String, CodingKey {
    case cpuCount
    case cpuCountSnake = "cpu_count"
    case vcpus
    case memoryGB
    case memoryGBSnake = "memory_gb"
    case memoryMiB = "memory_mib"
    case memoryBytes = "memory_bytes"
    case diskGB
    case diskGBSnake = "disk_gb"
    case diskBytes = "disk_bytes"
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    cpuCount =
      try container.decodeIfPresent(Int.self, forKey: .cpuCount)
      ?? container.decodeIfPresent(Int.self, forKey: .cpuCountSnake)
      ?? container.decodeIfPresent(Int.self, forKey: .vcpus)
    memoryGB =
      try container.decodeIfPresent(Int.self, forKey: .memoryGB)
      ?? container.decodeIfPresent(Int.self, forKey: .memoryGBSnake)
      ?? container.decodeIfPresent(Int.self, forKey: .memoryMiB).map { $0 / 1024 }
      ?? container.decodeIfPresent(Int.self, forKey: .memoryBytes).map { $0 / 1_073_741_824 }
    diskGB =
      try container.decodeIfPresent(Int.self, forKey: .diskGB)
      ?? container.decodeIfPresent(Int.self, forKey: .diskGBSnake)
      ?? container.decodeIfPresent(Int.self, forKey: .diskBytes).map { $0 / 1_073_741_824 }
  }

  var resources: VirtualMachine.Resources {
    .init(
      cpuCount: cpuCount ?? 0,
      memoryGB: memoryGB ?? 0,
      diskGB: diskGB ?? 0
    )
  }
}

struct DaemonBootMediaStatusDTO: Decodable {
  var vm: String
  var entries: [DaemonBootMediaStatusEntryDTO]

  var bootMediaStatus: BootMediaStatus {
    BootMediaStatus(
      vm: vm,
      entries: entries.map(\.bootMediaStatusEntry)
    )
  }
}

struct DaemonBootMediaStatusEntryDTO: Decodable {
  var kind: BootMediaStatusEntry.Kind
  var path: String
  var exists: Bool
  var sizeBytes: UInt64?
  var lastImport: DaemonBootMediaImportMetadataDTO?
  var lastVerification: DaemonBootMediaVerificationMetadataDTO?
  var lastDownloadPlan: DaemonBootMediaDownloadPlanMetadataDTO?
  var lastDownload: DaemonBootMediaDownloadResultMetadataDTO?

  enum CodingKeys: String, CodingKey {
    case kind
    case path
    case exists
    case sizeBytes = "size_bytes"
    case bytes
    case lastImport = "last_import"
    case lastVerification = "last_verification"
    case lastDownloadPlan = "last_download_plan"
    case lastDownload = "last_download"
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    kind = BootMediaStatusEntry.Kind(daemonValue: try container.decode(String.self, forKey: .kind))
    path = try container.decode(String.self, forKey: .path)
    exists = try container.decode(Bool.self, forKey: .exists)
    sizeBytes =
      try container.decodeIfPresent(UInt64.self, forKey: .sizeBytes)
      ?? container.decodeIfPresent(UInt64.self, forKey: .bytes)
    lastImport = try container.decodeIfPresent(
      DaemonBootMediaImportMetadataDTO.self, forKey: .lastImport)
    lastVerification = try container.decodeIfPresent(
      DaemonBootMediaVerificationMetadataDTO.self, forKey: .lastVerification)
    lastDownloadPlan = try container.decodeIfPresent(
      DaemonBootMediaDownloadPlanMetadataDTO.self, forKey: .lastDownloadPlan)
    lastDownload = try container.decodeIfPresent(
      DaemonBootMediaDownloadResultMetadataDTO.self, forKey: .lastDownload)
  }

  var bootMediaStatusEntry: BootMediaStatusEntry {
    BootMediaStatusEntry(
      kind: kind,
      path: path,
      exists: exists,
      sizeBytes: sizeBytes,
      lastImport: lastImport?.bootMediaImportMetadata,
      lastVerification: lastVerification?.bootMediaVerificationMetadata,
      lastDownloadPlan: lastDownloadPlan?.bootMediaDownloadPlanMetadata,
      lastDownload: lastDownload?.bootMediaDownloadResultMetadata
    )
  }
}

struct DaemonBootMediaImportMetadataDTO: Decodable {
  var vm: String
  var kind: BootMediaStatusEntry.Kind
  var source: String
  var destination: String
  var bytes: UInt64
  var replaced: Bool
  var importedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case vm
    case kind
    case source
    case destination
    case bytes
    case replaced
    case importedAtUnix = "imported_at_unix"
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    vm = try container.decode(String.self, forKey: .vm)
    kind = BootMediaStatusEntry.Kind(daemonValue: try container.decode(String.self, forKey: .kind))
    source = try container.decode(String.self, forKey: .source)
    destination = try container.decode(String.self, forKey: .destination)
    bytes = try container.decode(UInt64.self, forKey: .bytes)
    replaced = try container.decode(Bool.self, forKey: .replaced)
    importedAtUnix = try container.decode(UInt64.self, forKey: .importedAtUnix)
  }

  var bootMediaImportMetadata: BootMediaImportMetadata {
    BootMediaImportMetadata(
      vm: vm,
      kind: kind,
      source: source,
      destination: destination,
      bytes: bytes,
      replaced: replaced,
      importedAtUnix: importedAtUnix
    )
  }
}

struct DaemonBootMediaVerificationMetadataDTO: Decodable {
  var vm: String
  var kind: BootMediaStatusEntry.Kind
  var path: String
  var bytes: UInt64
  var expectedSHA256: String
  var actualSHA256: String
  var verified: Bool
  var verifiedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case vm
    case kind
    case path
    case bytes
    case expectedSHA256 = "expected_sha256"
    case actualSHA256 = "actual_sha256"
    case verified
    case verifiedAtUnix = "verified_at_unix"
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    vm = try container.decode(String.self, forKey: .vm)
    kind = BootMediaStatusEntry.Kind(daemonValue: try container.decode(String.self, forKey: .kind))
    path = try container.decode(String.self, forKey: .path)
    bytes = try container.decode(UInt64.self, forKey: .bytes)
    expectedSHA256 = try container.decode(String.self, forKey: .expectedSHA256)
    actualSHA256 = try container.decode(String.self, forKey: .actualSHA256)
    verified = try container.decode(Bool.self, forKey: .verified)
    verifiedAtUnix = try container.decode(UInt64.self, forKey: .verifiedAtUnix)
  }

  var bootMediaVerificationMetadata: BootMediaVerificationMetadata {
    BootMediaVerificationMetadata(
      vm: vm,
      kind: kind,
      path: path,
      bytes: bytes,
      expectedSHA256: expectedSHA256,
      actualSHA256: actualSHA256,
      verified: verified,
      verifiedAtUnix: verifiedAtUnix
    )
  }
}

struct DaemonBootMediaDownloadPlanMetadataDTO: Decodable {
  var vm: String
  var kind: BootMediaStatusEntry.Kind
  var url: String
  var destination: String
  var exists: Bool
  var bytes: UInt64?
  var expectedSHA256: String?
  var plannedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case vm
    case kind
    case url
    case destination
    case exists
    case bytes
    case expectedSHA256 = "expected_sha256"
    case plannedAtUnix = "planned_at_unix"
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    vm = try container.decode(String.self, forKey: .vm)
    kind = BootMediaStatusEntry.Kind(daemonValue: try container.decode(String.self, forKey: .kind))
    url = try container.decode(String.self, forKey: .url)
    destination = try container.decode(String.self, forKey: .destination)
    exists = try container.decode(Bool.self, forKey: .exists)
    bytes = try container.decodeIfPresent(UInt64.self, forKey: .bytes)
    expectedSHA256 = try container.decodeIfPresent(String.self, forKey: .expectedSHA256)
    plannedAtUnix = try container.decode(UInt64.self, forKey: .plannedAtUnix)
  }

  var bootMediaDownloadPlanMetadata: BootMediaDownloadPlanMetadata {
    BootMediaDownloadPlanMetadata(
      vm: vm,
      kind: kind,
      url: url,
      destination: destination,
      exists: exists,
      bytes: bytes,
      expectedSHA256: expectedSHA256,
      plannedAtUnix: plannedAtUnix
    )
  }
}

struct DaemonBootMediaDownloadResultMetadataDTO: Decodable {
  var vm: String
  var kind: BootMediaStatusEntry.Kind
  var url: String
  var destination: String
  var bytes: UInt64?
  var replaced: Bool
  var expectedSHA256: String?
  var actualSHA256: String?
  var verified: Bool?
  var downloaded: Bool
  var downloadedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case vm
    case kind
    case url
    case destination
    case bytes
    case replaced
    case expectedSHA256 = "expected_sha256"
    case actualSHA256 = "actual_sha256"
    case verified
    case downloaded
    case downloadedAtUnix = "downloaded_at_unix"
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    vm = try container.decode(String.self, forKey: .vm)
    kind = BootMediaStatusEntry.Kind(daemonValue: try container.decode(String.self, forKey: .kind))
    url = try container.decode(String.self, forKey: .url)
    destination = try container.decode(String.self, forKey: .destination)
    bytes = try container.decodeIfPresent(UInt64.self, forKey: .bytes)
    replaced = try container.decode(Bool.self, forKey: .replaced)
    expectedSHA256 = try container.decodeIfPresent(String.self, forKey: .expectedSHA256)
    actualSHA256 = try container.decodeIfPresent(String.self, forKey: .actualSHA256)
    verified = try container.decodeIfPresent(Bool.self, forKey: .verified)
    downloaded = try container.decode(Bool.self, forKey: .downloaded)
    downloadedAtUnix = try container.decode(UInt64.self, forKey: .downloadedAtUnix)
  }

  var bootMediaDownloadResultMetadata: BootMediaDownloadResultMetadata {
    BootMediaDownloadResultMetadata(
      vm: vm,
      kind: kind,
      url: url,
      destination: destination,
      bytes: bytes,
      replaced: replaced,
      expectedSHA256: expectedSHA256,
      actualSHA256: actualSHA256,
      verified: verified,
      downloaded: downloaded,
      downloadedAtUnix: downloadedAtUnix
    )
  }
}

struct DaemonGuestToolsStatusDTO: Decodable {
  var vm: String
  var tools: String
  var tokenCreatedAtUnix: UInt64
  var capabilities: [DaemonGuestToolsCapabilityDTO]
  var approvedSharedFolders: [DaemonGuestToolsApprovedSharedFolderDTO]
  var runtime: DaemonGuestToolsRuntimeDTO?

  enum CodingKeys: String, CodingKey {
    case vm
    case tools
    case tokenCreatedAtUnix = "token_created_at_unix"
    case capabilities
    case approvedSharedFolders = "approved_shared_folders"
    case runtime
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    vm = try container.decode(String.self, forKey: .vm)
    tools = try container.decode(String.self, forKey: .tools)
    tokenCreatedAtUnix = try container.decode(UInt64.self, forKey: .tokenCreatedAtUnix)
    capabilities = try container.decode([DaemonGuestToolsCapabilityDTO].self, forKey: .capabilities)
    approvedSharedFolders =
      try container.decodeIfPresent(
        [DaemonGuestToolsApprovedSharedFolderDTO].self,
        forKey: .approvedSharedFolders
      ) ?? []
    runtime = try container.decodeIfPresent(DaemonGuestToolsRuntimeDTO.self, forKey: .runtime)
  }

  var guestToolsStatus: GuestToolsStatus {
    GuestToolsStatus(
      vm: vm,
      tools: tools,
      tokenCreatedAtUnix: tokenCreatedAtUnix,
      capabilities: capabilities.map(\.guestToolsCapability),
      approvedSharedFolders: approvedSharedFolders.map(\.guestToolsApprovedSharedFolder),
      runtime: runtime?.guestToolsRuntime
    )
  }
}

struct DaemonGuestToolsCapabilityDTO: Decodable {
  var name: String
  var maxVersion: UInt16
  var enabledBy: String

  enum CodingKeys: String, CodingKey {
    case name
    case maxVersion = "max_version"
    case enabledBy = "enabled_by"
  }

  var guestToolsCapability: GuestToolsCapability {
    GuestToolsCapability(
      name: name,
      maxVersion: maxVersion,
      enabledBy: enabledBy
    )
  }
}

struct DaemonGuestToolsApprovedSharedFolderDTO: Decodable {
  var name: String
  var hostPath: String
  var hostPathToken: String
  var readOnly: Bool
  var approval: String

  enum CodingKeys: String, CodingKey {
    case name
    case hostPath = "host_path"
    case hostPathToken = "host_path_token"
    case readOnly = "read_only"
    case approval
  }

  var guestToolsApprovedSharedFolder: GuestToolsApprovedSharedFolder {
    GuestToolsApprovedSharedFolder(
      name: name,
      hostPath: hostPath,
      hostPathToken: hostPathToken,
      readOnly: readOnly,
      approval: approval
    )
  }
}

struct DaemonGuestToolsRuntimeDTO: Decodable {
  var connected: Bool
  var guestOS: String?
  var agentVersion: String?
  var capabilities: [String]
  var lastHeartbeatAtUnix: UInt64?
  var guestIPAddresses: [DaemonGuestToolsIPAddressDTO]
  var sharedFolders: [DaemonGuestToolsSharedFolderDTO]
  var metrics: DaemonGuestToolsMetricsDTO?
  var lastClipboard: DaemonGuestClipboardSnapshotDTO?
  var lastCommandResult: DaemonGuestToolsCommandResultDTO?
  var agentUpdate: DaemonGuestToolsAgentUpdateDTO?
  var updatedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case connected
    case guestOS = "guest_os"
    case agentVersion = "agent_version"
    case capabilities
    case lastHeartbeatAtUnix = "last_heartbeat_at_unix"
    case guestIPAddresses = "guest_ip_addresses"
    case sharedFolders = "shared_folders"
    case metrics
    case lastClipboard = "last_clipboard"
    case lastCommandResult = "last_command_result"
    case agentUpdate = "agent_update"
    case updatedAtUnix = "updated_at_unix"
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    connected = try container.decode(Bool.self, forKey: .connected)
    guestOS = try container.decodeIfPresent(String.self, forKey: .guestOS)
    agentVersion = try container.decodeIfPresent(String.self, forKey: .agentVersion)
    capabilities = try container.decode([String].self, forKey: .capabilities)
    lastHeartbeatAtUnix = try container.decodeIfPresent(UInt64.self, forKey: .lastHeartbeatAtUnix)
    guestIPAddresses = try container.decode(
      [DaemonGuestToolsIPAddressDTO].self, forKey: .guestIPAddresses)
    sharedFolders =
      try container.decodeIfPresent(
        [DaemonGuestToolsSharedFolderDTO].self,
        forKey: .sharedFolders
      ) ?? []
    metrics = try container.decodeIfPresent(DaemonGuestToolsMetricsDTO.self, forKey: .metrics)
    lastClipboard = try container.decodeIfPresent(
      DaemonGuestClipboardSnapshotDTO.self,
      forKey: .lastClipboard
    )
    lastCommandResult = try container.decodeIfPresent(
      DaemonGuestToolsCommandResultDTO.self,
      forKey: .lastCommandResult
    )
    agentUpdate = try container.decodeIfPresent(
      DaemonGuestToolsAgentUpdateDTO.self,
      forKey: .agentUpdate
    )
    updatedAtUnix = try container.decode(UInt64.self, forKey: .updatedAtUnix)
  }

  var guestToolsRuntime: GuestToolsRuntime {
    GuestToolsRuntime(
      connected: connected,
      guestOS: guestOS,
      agentVersion: agentVersion,
      capabilities: capabilities,
      lastHeartbeatAtUnix: lastHeartbeatAtUnix,
      guestIPAddresses: guestIPAddresses.map(\.guestToolsIPAddress),
      sharedFolders: sharedFolders.map(\.guestToolsSharedFolder),
      metrics: metrics?.guestToolsMetrics,
      lastClipboard: lastClipboard?.guestClipboardSnapshot,
      lastCommandResult: lastCommandResult?.guestToolsCommandResult,
      updatedAtUnix: updatedAtUnix,
      agentUpdate: agentUpdate?.guestToolsAgentUpdate
    )
  }
}

struct DaemonGuestClipboardSnapshotDTO: Decodable {
  var text: String
  var updatedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case text
    case updatedAtUnix = "updated_at_unix"
  }

  var guestClipboardSnapshot: GuestClipboardSnapshot {
    GuestClipboardSnapshot(
      text: text,
      updatedAtUnix: updatedAtUnix
    )
  }
}

struct DaemonGuestToolsAgentUpdateDTO: Decodable {
  var currentVersion: String
  var availableVersion: String
  var downloadURL: String?
  var signature: String?
  var observedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case currentVersion = "current_version"
    case availableVersion = "available_version"
    case downloadURL = "download_url"
    case signature
    case observedAtUnix = "observed_at_unix"
  }

  var guestToolsAgentUpdate: GuestToolsAgentUpdate {
    GuestToolsAgentUpdate(
      currentVersion: currentVersion,
      availableVersion: availableVersion,
      downloadURL: downloadURL,
      signature: signature,
      observedAtUnix: observedAtUnix
    )
  }
}

struct DaemonGuestToolsCommandResultDTO: Decodable {
  var requestID: String
  var capability: String?
  var ok: Bool
  var errorCode: String?
  var message: String?
  var result: GuestToolsCommandPayload?
  var metadata: GuestToolsCommandPayload?
  var completedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case requestID = "request_id"
    case capability
    case ok
    case errorCode = "error_code"
    case message
    case result
    case metadata
    case completedAtUnix = "completed_at_unix"
  }

  var guestToolsCommandResult: GuestToolsCommandResult {
    GuestToolsCommandResult(
      requestID: requestID,
      capability: capability,
      ok: ok,
      errorCode: errorCode,
      message: message,
      result: result,
      metadata: metadata,
      completedAtUnix: completedAtUnix
    )
  }
}

struct DaemonGuestToolsIPAddressDTO: Decodable {
  var address: String
  var interface: String?

  var guestToolsIPAddress: GuestToolsIPAddress {
    GuestToolsIPAddress(address: address, interface: interface)
  }
}

struct DaemonGuestToolsSharedFolderDTO: Decodable {
  var name: String
  var hostPathToken: String
  var mountedAtUnix: UInt64?

  enum CodingKeys: String, CodingKey {
    case name
    case hostPathToken = "host_path_token"
    case mountedAtUnix = "mounted_at_unix"
  }

  var guestToolsSharedFolder: GuestToolsSharedFolder {
    GuestToolsSharedFolder(
      name: name,
      hostPathToken: hostPathToken,
      mountedAtUnix: mountedAtUnix
    )
  }
}

struct DaemonGuestToolsMetricsDTO: Decodable {
  var cpuPercent: UInt8
  var memoryUsedMiB: UInt64
  var updatedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case cpuPercent = "cpu_percent"
    case memoryUsedMiB = "memory_used_mib"
    case updatedAtUnix = "updated_at_unix"
  }

  var guestToolsMetrics: GuestToolsMetrics {
    GuestToolsMetrics(
      cpuPercent: cpuPercent,
      memoryUsedMiB: memoryUsedMiB,
      updatedAtUnix: updatedAtUnix
    )
  }
}

struct DaemonRunnerMetadataDTO: Decodable {
  var engine: String
  var pid: UInt32?
  var command: [String]
  var logPath: String
  var startedAtUnix: UInt64
  var dryRun: Bool
  var launchSpecPath: String?
  var launchReadiness: DaemonLaunchReadinessDTO?
  var guestTools: DaemonGuestToolsRunnerMetadataDTO?

  enum CodingKeys: String, CodingKey {
    case engine
    case pid
    case command
    case logPath = "log_path"
    case startedAtUnix = "started_at_unix"
    case dryRun = "dry_run"
    case launchSpecPath = "launch_spec_path"
    case launchReadiness = "launch_readiness"
    case guestTools = "guest_tools"
  }

  func runnerStatus(qmpSupervisor: QMPSupervisor? = nil) -> RunnerStatus {
    RunnerStatus(
      engine: engine,
      pid: pid,
      command: command,
      logPath: logPath,
      startedAtUnix: startedAtUnix,
      dryRun: dryRun,
      launchSpecPath: launchSpecPath,
      launchReadiness: launchReadiness?.launchReadiness,
      qmpSupervisor: qmpSupervisor,
      guestTools: guestTools?.guestToolsRunnerStatus
    )
  }
}

struct DaemonGuestToolsRunnerMetadataDTO: Decodable {
  var transport: String
  var channelName: String
  var socketPath: String
  var tokenPath: String
  var tokenCreatedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case transport
    case channelName = "channel_name"
    case socketPath = "socket_path"
    case tokenPath = "token_path"
    case tokenCreatedAtUnix = "token_created_at_unix"
  }

  var guestToolsRunnerStatus: GuestToolsRunnerStatus {
    GuestToolsRunnerStatus(
      transport: transport,
      channelName: channelName,
      socketPath: socketPath,
      tokenPath: tokenPath,
      tokenCreatedAtUnix: tokenCreatedAtUnix
    )
  }
}

struct DaemonSnapshotPreflightStatusDTO: Decodable {
  var vm: String
  var consistency: SnapshotConsistency
  var backendFreezeThawSupported: Bool?
  var guestToolsConnected: Bool?
  var capabilities: [String]?
  var connected: Bool?
  var availableCapabilities: [String]?
  var ready: Bool
  var blockers: [DaemonSnapshotPreflightBlockerDTO]?
  var checkedAtUnix: UInt64?
  var preparedAtUnix: UInt64?

  enum CodingKeys: String, CodingKey {
    case vm
    case consistency
    case backendFreezeThawSupported = "backend_freeze_thaw_supported"
    case guestToolsConnected = "guest_tools_connected"
    case capabilities
    case connected
    case availableCapabilities = "available_capabilities"
    case ready
    case blockers
    case checkedAtUnix = "checked_at_unix"
    case preparedAtUnix = "prepared_at_unix"
  }

  var snapshotPreflightStatus: SnapshotPreflightStatus {
    let isConnected = guestToolsConnected ?? connected ?? false
    let reportedCapabilities = capabilities ?? availableCapabilities ?? []
    let reportedBlockers =
      blockers?.map(\.snapshotPreflightBlocker)
      ?? inferredSnapshotPreflightBlockers(
        backendSupported: backendFreezeThawSupported ?? false,
        guestToolsConnected: isConnected,
        ready: ready
      )
    return SnapshotPreflightStatus(
      vm: vm,
      consistency: consistency,
      backendFreezeThawSupported: backendFreezeThawSupported ?? false,
      guestToolsConnected: isConnected,
      capabilities: reportedCapabilities,
      ready: ready,
      blockers: reportedBlockers,
      checkedAtUnix: checkedAtUnix ?? preparedAtUnix
    )
  }
}

private func inferredSnapshotPreflightBlockers(
  backendSupported: Bool,
  guestToolsConnected: Bool,
  ready: Bool
) -> [SnapshotPreflightBlocker] {
  guard !ready else {
    return []
  }

  var blockers: [SnapshotPreflightBlocker] = []
  if !backendSupported {
    blockers.append(
      SnapshotPreflightBlocker(
        code: "backend-freeze-thaw-unavailable",
        message:
          "Freeze/thaw orchestration requires the bridgevmd-owned running backend; this offline preflight cannot drive the guest agent.",
        path: nil
      ))
  }
  if !guestToolsConnected {
    blockers.append(
      SnapshotPreflightBlocker(
        code: "guest-tools-not-connected",
        message: "Guest tools must be connected before application-consistent preflight can pass.",
        path: nil
      ))
  }
  return blockers
}

struct DaemonSnapshotPreflightBlockerDTO: Decodable {
  var code: String
  var message: String
  var path: String?

  var snapshotPreflightBlocker: SnapshotPreflightBlocker {
    SnapshotPreflightBlocker(code: code, message: message, path: path)
  }
}

struct ApplicationConsistentSnapshotExecution: Equatable {
  var vm: String
  var snapshot: String
  var freezeRequestID: String
  var thawRequestID: String
  var pendingCommandsAfterFreeze: Int
  var pendingCommandsAfterThaw: Int
  var snapshotCreatedAtUnix: UInt64
  var freezeResult: ApplicationConsistentSnapshotCommandResult
  var thawResult: ApplicationConsistentSnapshotCommandResult
  var preflightReady: Bool
  var note: String

  var summaryTitle: String {
    guard preflightReady else {
      return "Preflight blocked"
    }

    return freezeResult.ok && thawResult.ok ? "Snapshot executed" : "Freeze/thaw incomplete"
  }
}

struct ApplicationConsistentSnapshotCommandResult: Equatable {
  var requestID: String
  var capability: String?
  var ok: Bool
  var errorCode: String?
  var message: String?
  var completedAtUnix: UInt64

  var statusTitle: String {
    if ok {
      return "OK"
    }

    return errorCode ?? "Failed"
  }
}

struct DaemonApplicationConsistentSnapshotExecutionDTO: Decodable {
  var vm: String
  var snapshot: String
  var freezeRequestID: String
  var thawRequestID: String
  var pendingCommandsAfterFreeze: Int
  var pendingCommandsAfterThaw: Int
  var snapshotCreatedAtUnix: UInt64
  var freezeResult: DaemonApplicationConsistentSnapshotCommandResultDTO
  var thawResult: DaemonApplicationConsistentSnapshotCommandResultDTO
  var preflightReady: Bool
  var note: String

  enum CodingKeys: String, CodingKey {
    case vm
    case snapshot
    case freezeRequestID = "freeze_request_id"
    case thawRequestID = "thaw_request_id"
    case pendingCommandsAfterFreeze = "pending_commands_after_freeze"
    case pendingCommandsAfterThaw = "pending_commands_after_thaw"
    case snapshotCreatedAtUnix = "snapshot_created_at_unix"
    case freezeResult = "freeze_result"
    case thawResult = "thaw_result"
    case preflightReady = "preflight_ready"
    case note
  }

  var applicationConsistentSnapshotExecution: ApplicationConsistentSnapshotExecution {
    ApplicationConsistentSnapshotExecution(
      vm: vm,
      snapshot: snapshot,
      freezeRequestID: freezeRequestID,
      thawRequestID: thawRequestID,
      pendingCommandsAfterFreeze: pendingCommandsAfterFreeze,
      pendingCommandsAfterThaw: pendingCommandsAfterThaw,
      snapshotCreatedAtUnix: snapshotCreatedAtUnix,
      freezeResult: freezeResult.applicationConsistentSnapshotCommandResult,
      thawResult: thawResult.applicationConsistentSnapshotCommandResult,
      preflightReady: preflightReady,
      note: note
    )
  }
}

struct DaemonApplicationConsistentSnapshotCommandResultDTO: Decodable {
  var requestID: String
  var capability: String?
  var ok: Bool
  var errorCode: String?
  var message: String?
  var completedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case requestID = "request_id"
    case capability
    case ok
    case errorCode = "error_code"
    case message
    case completedAtUnix = "completed_at_unix"
  }

  var applicationConsistentSnapshotCommandResult: ApplicationConsistentSnapshotCommandResult {
    ApplicationConsistentSnapshotCommandResult(
      requestID: requestID,
      capability: capability,
      ok: ok,
      errorCode: errorCode,
      message: message,
      completedAtUnix: completedAtUnix
    )
  }
}

enum RuntimeResourceVisibility: String, Codable, CaseIterable, Equatable {
  case foreground
  case background

  var title: String {
    switch self {
    case .foreground:
      return "Foreground"
    case .background:
      return "Background"
    }
  }

  var systemImage: String {
    switch self {
    case .foreground:
      return "rectangle.inset.filled"
    case .background:
      return "rectangle.stack"
    }
  }
}

struct RuntimeResourcePolicy: Equatable {
  var vm: String
  var mode: String
  var profile: String
  var visibility: RuntimeResourceVisibility
  var state: String
  var onBattery: Bool
  var memory: String
  var cpu: String
  var displayFPSCap: String
  var rationale: String
  var liveApplied: Bool
  var liveApplyBlockers: [RuntimeResourcePolicyBlocker]
  var updatedAtUnix: UInt64

  var liveApplyTitle: String {
    if liveApplied {
      return "Applied"
    }
    if !liveApplyBlockers.isEmpty {
      return "Blocked"
    }
    return "Recorded"
  }

  var liveApplyBlockerSummary: String? {
    let value = liveApplyBlockers.map(\.summary).joined(separator: "; ")
    return value.isEmpty ? nil : value
  }
}

struct RuntimeResourcePolicyBlocker: Equatable {
  var code: String
  var message: String

  var summary: String {
    "\(code): \(message)"
  }
}

struct DaemonRuntimeResourcePolicyDTO: Decodable {
  var vm: String
  var mode: String
  var profile: String
  var visibility: RuntimeResourceVisibility
  var state: String
  var onBattery: Bool
  var memory: String
  var cpu: String
  var displayFPSCap: String
  var rationale: String
  var liveApplied: Bool
  var liveApplyBlockers: [DaemonRuntimeResourcePolicyBlockerDTO]
  var updatedAtUnix: UInt64

  enum CodingKeys: String, CodingKey {
    case vm
    case mode
    case profile
    case visibility
    case state
    case onBattery = "on_battery"
    case memory
    case cpu
    case displayFPSCap = "display_fps_cap"
    case rationale
    case liveApplied = "live_applied"
    case liveApplyBlockers = "live_apply_blockers"
    case updatedAtUnix = "updated_at_unix"
  }

  var runtimeResourcePolicy: RuntimeResourcePolicy {
    RuntimeResourcePolicy(
      vm: vm,
      mode: mode,
      profile: profile,
      visibility: visibility,
      state: state,
      onBattery: onBattery,
      memory: memory,
      cpu: cpu,
      displayFPSCap: displayFPSCap,
      rationale: rationale,
      liveApplied: liveApplied,
      liveApplyBlockers: liveApplyBlockers.map(\.runtimeResourcePolicyBlocker),
      updatedAtUnix: updatedAtUnix
    )
  }
}

struct DaemonRuntimeResourcePolicyBlockerDTO: Decodable {
  var code: String
  var message: String

  var runtimeResourcePolicyBlocker: RuntimeResourcePolicyBlocker {
    RuntimeResourcePolicyBlocker(code: code, message: message)
  }
}

struct DaemonLaunchReadinessDTO: Decodable {
  var ready: Bool
  var blockers: [DaemonLaunchReadinessBlockerDTO]

  var launchReadiness: LaunchReadiness {
    LaunchReadiness(
      ready: ready,
      blockers: blockers.map(\.launchReadinessBlocker)
    )
  }
}

struct DaemonLaunchReadinessBlockerDTO: Decodable {
  var code: String
  var message: String
  var path: String?
  var capability: String?

  var launchReadinessBlocker: LaunchReadinessBlocker {
    LaunchReadinessBlocker(code: code, message: message, path: path, capability: capability)
  }
}

struct DaemonReadinessReportDTO: Decodable {
  var vm: String
  private var modeValue: String
  private var stateValue: String
  var metadataOnly: Bool
  var liveE2ERequired: Bool
  var liveEvidence: DaemonLiveEvidenceDTO?
  var evidenceRequirements: [DaemonEvidenceRequirementDTO]
  var bootMedia: DaemonBootMediaStatusDTO?
  var bootMediaError: String?
  var snapshotChain: DaemonSnapshotChainDTO?
  var snapshotChainError: String?
  var runner: DaemonRunnerMetadataDTO?
  var runnerError: String?
  var preRunLaunchReadiness: DaemonLaunchReadinessDTO?
  var qmpSupervisor: DaemonQMPSupervisorDTO?
  var blockers: [String]
  var notes: [String]

  enum CodingKeys: String, CodingKey {
    case vm
    case mode
    case state
    case metadataOnly = "metadata_only"
    case liveE2ERequired = "live_e2e_required"
    case liveEvidence = "live_evidence"
    case evidenceRequirements = "evidence_requirements"
    case bootMedia = "boot_media"
    case bootMediaError = "boot_media_error"
    case snapshotChain = "snapshot_chain"
    case snapshotChainError = "snapshot_chain_error"
    case runner
    case runnerError = "runner_error"
    case preRunLaunchReadiness = "pre_run_launch_readiness"
    case qmpSupervisor = "qmp_supervisor"
    case blockers
    case notes
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    vm = try container.decode(String.self, forKey: .vm)
    modeValue = try container.decode(String.self, forKey: .mode)
    stateValue = try container.decode(String.self, forKey: .state)
    metadataOnly = try container.decode(Bool.self, forKey: .metadataOnly)
    liveE2ERequired = try container.decode(Bool.self, forKey: .liveE2ERequired)
    liveEvidence = try container.decodeIfPresent(DaemonLiveEvidenceDTO.self, forKey: .liveEvidence)
    evidenceRequirements = try container.decodeIfPresent(
      [DaemonEvidenceRequirementDTO].self,
      forKey: .evidenceRequirements
    ) ?? []
    bootMedia = try container.decodeIfPresent(DaemonBootMediaStatusDTO.self, forKey: .bootMedia)
    bootMediaError = try container.decodeIfPresent(String.self, forKey: .bootMediaError)
    snapshotChain = try container.decodeIfPresent(
      DaemonSnapshotChainDTO.self,
      forKey: .snapshotChain
    )
    snapshotChainError = try container.decodeIfPresent(String.self, forKey: .snapshotChainError)
    runner = try container.decodeIfPresent(DaemonRunnerMetadataDTO.self, forKey: .runner)
    runnerError = try container.decodeIfPresent(String.self, forKey: .runnerError)
    preRunLaunchReadiness = try container.decodeIfPresent(
      DaemonLaunchReadinessDTO.self,
      forKey: .preRunLaunchReadiness
    )
    qmpSupervisor = try container.decodeIfPresent(DaemonQMPSupervisorDTO.self, forKey: .qmpSupervisor)
    blockers = try container.decode([String].self, forKey: .blockers)
    notes = try container.decode([String].self, forKey: .notes)
  }

  var vmReadinessReport: VMReadinessReport {
    VMReadinessReport(
      vm: vm,
      mode: VirtualMachine.EngineMode(daemonValue: modeValue),
      state: VirtualMachine.Status(daemonValue: stateValue),
      metadataOnly: metadataOnly,
      liveE2ERequired: liveE2ERequired,
      liveEvidence: liveEvidence?.vmLiveEvidence,
      evidenceRequirements: evidenceRequirements.map(\.vmEvidenceRequirement),
      bootMedia: bootMedia?.bootMediaStatus,
      bootMediaError: bootMediaError,
      snapshotChain: snapshotChain?.vmSnapshotChain,
      snapshotChainError: snapshotChainError,
      runner: runner?.runnerStatus(),
      runnerError: runnerError,
      preRunLaunchReadiness: preRunLaunchReadiness?.launchReadiness,
      qmpSupervisor: qmpSupervisor?.qmpSupervisor,
      blockers: blockers,
      notes: notes
    )
  }
}

struct DaemonLiveEvidenceDTO: Decodable {
  var path: String
  var backend: String
  var vmName: String
  var bootMode: String
  var diskFormat: String
  var network: String
  var serialSentinelRequired: Bool
  var serialSentinelProven: Bool
  var graphicalBootProgressProven: Bool
  var viewerEvidenceProven: Bool
  var qmpEvidenceProven: Bool
  var guestToolsEffectsProven: Bool
  var summary: String

  enum CodingKeys: String, CodingKey {
    case path
    case backend
    case vmName = "vm_name"
    case bootMode = "boot_mode"
    case diskFormat = "disk_format"
    case network
    case serialSentinelRequired = "serial_sentinel_required"
    case serialSentinelProven = "serial_sentinel_proven"
    case graphicalBootProgressProven = "graphical_boot_progress_proven"
    case viewerEvidenceProven = "viewer_evidence_proven"
    case qmpEvidenceProven = "qmp_evidence_proven"
    case guestToolsEffectsProven = "guest_tools_effects_proven"
    case summary
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    path = try container.decode(String.self, forKey: .path)
    backend = try container.decode(String.self, forKey: .backend)
    vmName = try container.decode(String.self, forKey: .vmName)
    bootMode = try container.decode(String.self, forKey: .bootMode)
    diskFormat = try container.decode(String.self, forKey: .diskFormat)
    network = try container.decode(String.self, forKey: .network)
    serialSentinelRequired = try container.decode(Bool.self, forKey: .serialSentinelRequired)
    serialSentinelProven = try container.decode(Bool.self, forKey: .serialSentinelProven)
    graphicalBootProgressProven =
      try container.decodeIfPresent(Bool.self, forKey: .graphicalBootProgressProven) ?? false
    viewerEvidenceProven =
      try container.decodeIfPresent(Bool.self, forKey: .viewerEvidenceProven) ?? false
    qmpEvidenceProven =
      try container.decodeIfPresent(Bool.self, forKey: .qmpEvidenceProven) ?? false
    guestToolsEffectsProven =
      try container.decodeIfPresent(Bool.self, forKey: .guestToolsEffectsProven) ?? false
    summary = try container.decode(String.self, forKey: .summary)
  }

  var vmLiveEvidence: VMLiveEvidence {
    VMLiveEvidence(
      path: path,
      backend: backend,
      vmName: vmName,
      bootMode: bootMode,
      diskFormat: diskFormat,
      network: network,
      serialSentinelRequired: serialSentinelRequired,
      serialSentinelProven: serialSentinelProven,
      graphicalBootProgressProven: graphicalBootProgressProven,
      viewerEvidenceProven: viewerEvidenceProven,
      qmpEvidenceProven: qmpEvidenceProven,
      guestToolsEffectsProven: guestToolsEffectsProven,
      summary: summary
    )
  }
}

struct DaemonEvidenceRequirementDTO: Decodable {
  var kind: String
  var required: Bool
  var proven: Bool
  var note: String

  var vmEvidenceRequirement: VMEvidenceRequirement {
    VMEvidenceRequirement(kind: kind, required: required, proven: proven, note: note)
  }
}

extension BootMediaStatusEntry.Kind {
  fileprivate init(daemonValue: String) {
    switch daemonValue {
    case "InstallerImage", "installer-image", "installer_image":
      self = .installerImage
    case "Kernel", "kernel":
      self = .kernel
    case "Initrd", "initrd":
      self = .initrd
    case "MacosRestoreImage", "macos-restore-image", "macos_restore_image":
      self = .macosRestoreImage
    default:
      self = .unknown
    }
  }
}

extension VirtualMachine.Status {
  fileprivate init(daemonValue: String?) {
    switch daemonValue?.lowercased() {
    case "stopped":
      self = .stopped
    case "running":
      self = .running
    case "paused":
      self = .paused
    case "suspended":
      self = .suspended
    case "error", "failed":
      self = .error
    default:
      self = .error
    }
  }
}

extension VirtualMachine.EngineMode {
  fileprivate init(daemonValue: String?) {
    switch daemonValue?.lowercased() {
    case "compatibility", "compatibility_mode", "qemu":
      self = .compatibility
    default:
      self = .fast
    }
  }
}

extension KeyedDecodingContainer {
  fileprivate func decodeFlexibleDate(keys: [Key]) throws -> Date? {
    for key in keys {
      if let timestamp = try decodeIfPresent(TimeInterval.self, forKey: key) {
        return Date(timeIntervalSince1970: timestamp)
      }

      guard let value = try decodeIfPresent(String.self, forKey: key) else {
        continue
      }

      if let timestamp = TimeInterval(value) {
        return Date(timeIntervalSince1970: timestamp)
      }

      if let date = ISO8601DateFormatter.bridgeVMWithFractionalSeconds.date(from: value)
        ?? ISO8601DateFormatter.bridgeVM.date(from: value)
      {
        return date
      }
    }

    return nil
  }
}

extension ISO8601DateFormatter {
  fileprivate static let bridgeVM: ISO8601DateFormatter = {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime]
    return formatter
  }()

  fileprivate static let bridgeVMWithFractionalSeconds: ISO8601DateFormatter = {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return formatter
  }()
}

actor MockVirtualMachineClient: VirtualMachineClient, VirtualMachineClientSourceProviding {
  private let bootTemplates: [BootTemplate] = [
    BootTemplate(
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
      note: "Place the installer image inside the .vmbridge bundle."
    ),
    BootTemplate(
      id: "fedora-arm64-installer",
      guestOS: "fedora",
      guestVersion: nil,
      guestArch: "arm64",
      mode: .linuxInstaller,
      mediaLabel: "fedora arm64 installer image",
      source: "manual",
      installerImage: "installers/fedora-arm64.iso",
      kernelPath: nil,
      initrdPath: nil,
      kernelCommandLine: nil,
      macosRestoreImage: nil,
      note: "Place the installer image inside the .vmbridge bundle."
    ),
    BootTemplate(
      id: "debian-arm64-installer",
      guestOS: "debian",
      guestVersion: nil,
      guestArch: "arm64",
      mode: .linuxInstaller,
      mediaLabel: "debian arm64 installer image",
      source: "manual",
      installerImage: "installers/debian-arm64.iso",
      kernelPath: nil,
      initrdPath: nil,
      kernelCommandLine: nil,
      macosRestoreImage: nil,
      note: "Place the installer image inside the .vmbridge bundle."
    ),
    BootTemplate(
      id: "macos-restore",
      guestOS: "macos",
      guestVersion: nil,
      guestArch: "arm64",
      mode: .macosRestore,
      mediaLabel: "macOS restore image",
      source: "manual",
      installerImage: nil,
      kernelPath: nil,
      initrdPath: nil,
      kernelCommandLine: nil,
      macosRestoreImage: "installers/macos-restore.ipsw",
      note: "Place a macOS restore image inside the .vmbridge bundle."
    ),
  ]

  private var virtualMachines: [VirtualMachine] = [
    VirtualMachine(
      id: UUID(uuidString: "6FCA05BF-5635-4923-BDC7-9D37D4EA4B4E") ?? UUID(),
      name: "Windows 11 Arm Dev",
      guest: "Windows 11 Arm",
      status: .running,
      mode: .fast,
      resources: .init(cpuCount: 6, memoryGB: 12, diskGB: 128),
      uptime: "2h 14m",
      ipAddress: "192.168.64.12",
      lastStarted: Date(timeIntervalSinceNow: -8_040),
      notes: "Fast Mode candidate for daily app testing."
    ),
    VirtualMachine(
      id: UUID(uuidString: "3122800D-EC31-4C62-AAD8-B1510DA3CFA7") ?? UUID(),
      name: "Ubuntu Arm64 Lab",
      guest: "Ubuntu 24.04 Arm64",
      status: .paused,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "47m",
      ipAddress: "192.168.64.23",
      lastStarted: Date(timeIntervalSinceNow: -2_820),
      notes: "Package build environment with shared folder enabled."
    ),
    VirtualMachine(
      id: UUID(uuidString: "24C8350A-2DD4-481B-9429-F9E6699DC397") ?? UUID(),
      name: "Legacy Linux QEMU",
      guest: "Debian x86_64",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 2, memoryGB: 4, diskGB: 40),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: Date(timeIntervalSinceNow: -86_400),
      notes: "Compatibility Mode profile for x86 testing."
    ),
    VirtualMachine(
      id: UUID(uuidString: "44F1CD45-771E-4081-9D8D-9D178796670C") ?? UUID(),
      name: "macOS Preview",
      guest: "macOS Arm Guest",
      status: .suspended,
      mode: .fast,
      resources: .init(cpuCount: 4, memoryGB: 10, diskGB: 96),
      uptime: "Suspended",
      ipAddress: nil,
      lastStarted: Date(timeIntervalSinceNow: -172_800),
      notes: "Saved state is ready for quick resume."
    ),
  ]
  private var bootMediaImports: [VirtualMachine.ID: BootMediaImportMetadata] = [:]
  private var bootMediaVerifications: [VirtualMachine.ID: BootMediaVerificationMetadata] = [:]
  private var bootMediaDownloadPlans: [VirtualMachine.ID: BootMediaDownloadPlanMetadata] = [:]
  private var bootMediaDownloads: [VirtualMachine.ID: BootMediaDownloadResultMetadata] = [:]
  private var portForwardsByID: [VirtualMachine.ID: [VMPortForward]] = [:]
  private var sharedFoldersByID: [VirtualMachine.ID: [VMSharedFolder]] = [:]
  private var mountedSharedFolderNames: [VirtualMachine.ID: Set<String>] = [:]
  private var diskPreparationsByID: [VirtualMachine.ID: DiskPreparation] = [:]
  private var snapshotsByID: [VirtualMachine.ID: [VMSnapshot]] = [:]
  private var snapshotDisksByID: [VirtualMachine.ID: [VMSnapshotDisk]] = [:]
  private var runtimeResourcePoliciesByID: [VirtualMachine.ID: RuntimeResourcePolicy] = [:]

  nonisolated var sourceTitle: String {
    "Mock inventory"
  }

  func listVirtualMachines() async throws -> [VirtualMachine] {
    try await Task.sleep(nanoseconds: 180_000_000)
    return virtualMachines
  }

  func inspectStoreDoctor() async throws -> StoreDoctorReport {
    try await Task.sleep(nanoseconds: 80_000_000)
    return StoreDoctorReport(
      storeRoot: "~/Library/Application Support/BridgeVM",
      vmsDir: "~/Library/Application Support/BridgeVM/vms",
      status: "MOCK",
      source: sourceTitle
    )
  }

  func listBootTemplates() async throws -> [BootTemplate] {
    try await Task.sleep(nanoseconds: 120_000_000)
    return bootTemplates
  }

  func inspectReadinessReport(on id: VirtualMachine.ID) async throws -> VMReadinessReport {
    try await Task.sleep(nanoseconds: 120_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    async let bootMedia = inspectBootMediaStatus(on: id)
    async let snapshotChain = inspectSnapshotChain(on: id)
    async let runner = inspectRunnerStatus(on: id)
    let liveEvidenceRequired = vm.status == .running || vm.status == .paused || vm.status == .suspended
    let preRunReadiness =
      vm.status == .stopped || vm.status == .error
      ? LaunchReadiness(
        ready: true,
        blockers: []
      )
      : nil

    return VMReadinessReport(
      vm: vm.name,
      mode: vm.mode,
      state: vm.status,
      metadataOnly: true,
      liveE2ERequired: liveEvidenceRequired,
      liveEvidence: nil,
      evidenceRequirements: [
        VMEvidenceRequirement(
          kind: "live-boot",
          required: liveEvidenceRequired,
          proven: false,
          note: "Mock inventory exposes metadata readiness only."
        ),
        VMEvidenceRequirement(
          kind: "console",
          required: liveEvidenceRequired,
          proven: false,
          note: "Mock inventory does not preserve graphical or serial evidence."
        ),
        VMEvidenceRequirement(
          kind: "guest-tools-effects",
          required: liveEvidenceRequired,
          proven: false,
          note: "Mock inventory does not prove live guest-tools side effects."
        ),
      ],
      bootMedia: try await bootMedia,
      bootMediaError: nil,
      snapshotChain: try await snapshotChain,
      snapshotChainError: nil,
      runner: try await runner,
      runnerError: nil,
      preRunLaunchReadiness: preRunReadiness,
      blockers: [],
      notes: [
        "Mock readiness report; use bridgevmd for authoritative host evidence."
      ]
    )
  }

  func recommendMode(for choice: GuestChoice) async throws -> ModeRecommendation {
    try await Task.sleep(nanoseconds: 120_000_000)

    let os = choice.os.lowercased()
    let arch = choice.arch.lowercased()
    let version = choice.version?.lowercased() ?? ""
    let supportedFastGuest =
      ["ubuntu", "fedora", "debian", "macos"].contains(os)
      && ["arm64", "aarch64"].contains(arch)
    let windows11Arm =
      os == "windows" && version.hasPrefix("11") && ["arm64", "aarch64"].contains(arch)

    if supportedFastGuest {
      return ModeRecommendation(
        mode: .fast,
        performance: "High",
        batteryImpact: "Low",
        integration: "Full when BridgeVM Tools are installed",
        message: "Native optimized path available on Apple Silicon.",
        fastModeAvailable: true,
        bootTemplate: bootTemplates.first {
          $0.guestOS.lowercased() == os && $0.guestArch.lowercased() == arch
        }
      )
    }

    if windows11Arm {
      return ModeRecommendation(
        mode: .fast,
        performance: "High for productivity workloads",
        batteryImpact: "Low to medium",
        integration: "Experimental",
        message:
          "Windows 11 Arm can use Fast Mode Experimental with a restricted backend. BridgeVM must not claim Microsoft-authorized status.",
        fastModeAvailable: true,
        bootTemplate: nil
      )
    }

    return ModeRecommendation(
      mode: .compatibility,
      performance: arch == "x86_64" ? "Medium to low on Apple Silicon" : "Medium",
      batteryImpact: "Higher",
      integration: "Limited or partial",
      message:
        "Fast Mode is not available for this operating system. Use Compatibility Mode instead.",
      fastModeAvailable: false,
      bootTemplate: nil
    )
  }

  func inspectBootMediaStatus(on id: VirtualMachine.ID) async throws -> BootMediaStatus {
    try await Task.sleep(nanoseconds: 160_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let defaultKind: BootMediaStatusEntry.Kind =
      vm.guest.localizedCaseInsensitiveContains("macOS")
      ? .macosRestoreImage
      : .installerImage
    let imported = bootMediaImports[id]
    let verification = bootMediaVerifications[id]
    let downloadPlan = bootMediaDownloadPlans[id]
    let download = bootMediaDownloads[id]
    let kind = imported?.kind ?? defaultKind
    let fileName =
      kind == .macosRestoreImage
      ? "macos-restore.ipsw"
      : "\(vm.name.lowercased().replacingOccurrences(of: " ", with: "-")).iso"
    let exists = imported != nil || download?.downloaded == true || vm.mode == .fast

    return BootMediaStatus(
      vm: vm.name,
      entries: [
        BootMediaStatusEntry(
          kind: kind,
          path: imported?.destination ?? download?.destination ?? "installers/\(fileName)",
          exists: exists,
          sizeBytes: imported?.bytes ?? download?.bytes ?? (exists ? 5_368_709_120 : nil),
          lastImport: imported
            ?? (exists
              ? BootMediaImportMetadata(
                vm: vm.name,
                kind: kind,
                source: "/Downloads/\(fileName)",
                destination: "installers/\(fileName)",
                bytes: 5_368_709_120,
                replaced: false,
                importedAtUnix: UInt64(Date(timeIntervalSinceNow: -86_400).timeIntervalSince1970)
              ) : nil),
          lastVerification: verification
            ?? (exists
              ? BootMediaVerificationMetadata(
                vm: vm.name,
                kind: kind,
                path: imported?.destination ?? "installers/\(fileName)",
                bytes: imported?.bytes ?? 5_368_709_120,
                expectedSHA256: "mock",
                actualSHA256: "mock",
                verified: true,
                verifiedAtUnix: UInt64(Date(timeIntervalSinceNow: -7_200).timeIntervalSince1970)
              ) : nil),
          lastDownloadPlan: downloadPlan,
          lastDownload: download
        )
      ]
    )
  }

  func importBootMedia(
    sourcePath: String,
    kind requestedKind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaImportMetadata {
    try await Task.sleep(nanoseconds: 220_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let kind =
      requestedKind?.isImportable == true
      ? requestedKind ?? .installerImage
      : (vm.guest.localizedCaseInsensitiveContains("macOS") ? .macosRestoreImage : .installerImage)
    let sourceName = URL(fileURLWithPath: sourcePath).lastPathComponent
    let destinationName =
      sourceName.isEmpty
      ? (kind == .macosRestoreImage
        ? "macos-restore.ipsw"
        : "\(vm.name.lowercased().replacingOccurrences(of: " ", with: "-")).iso")
      : sourceName
    let metadata = BootMediaImportMetadata(
      vm: vm.name,
      kind: kind,
      source: sourcePath,
      destination: "installers/\(destinationName)",
      bytes: 5_368_709_120,
      replaced: bootMediaImports[id] != nil,
      importedAtUnix: UInt64(Date().timeIntervalSince1970)
    )
    bootMediaImports[id] = metadata
    return metadata
  }

  func verifyBootMedia(
    expectedSHA256: String,
    kind requestedKind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaVerificationMetadata {
    try await Task.sleep(nanoseconds: 180_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let imported = bootMediaImports[id]
    let kind =
      requestedKind?.isImportable == true
      ? requestedKind ?? .installerImage
      : (imported?.kind
        ?? (vm.guest.localizedCaseInsensitiveContains("macOS")
          ? .macosRestoreImage : .installerImage))
    let fileName =
      kind == .macosRestoreImage
      ? "macos-restore.ipsw"
      : "\(vm.name.lowercased().replacingOccurrences(of: " ", with: "-")).iso"
    let metadata = BootMediaVerificationMetadata(
      vm: vm.name,
      kind: kind,
      path: imported?.destination ?? "installers/\(fileName)",
      bytes: imported?.bytes ?? 5_368_709_120,
      expectedSHA256: expectedSHA256,
      actualSHA256: expectedSHA256,
      verified: true,
      verifiedAtUnix: UInt64(Date().timeIntervalSince1970)
    )
    bootMediaVerifications[id] = metadata
    return metadata
  }

  func planBootMediaDownload(
    url: String,
    expectedSHA256: String?,
    kind requestedKind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaDownloadPlanMetadata {
    try await Task.sleep(nanoseconds: 160_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let kind =
      requestedKind?.isImportable == true
      ? requestedKind ?? .installerImage
      : (vm.guest.localizedCaseInsensitiveContains("macOS") ? .macosRestoreImage : .installerImage)
    let urlFileName = URL(string: url)?.lastPathComponent ?? ""
    let fileName =
      urlFileName.isEmpty
      ? "\(vm.name.lowercased().replacingOccurrences(of: " ", with: "-")).iso"
      : urlFileName
    let metadata = BootMediaDownloadPlanMetadata(
      vm: vm.name,
      kind: kind,
      url: url,
      destination: "installers/\(fileName)",
      exists: bootMediaImports[id] != nil || vm.mode == .fast,
      bytes: bootMediaImports[id]?.bytes,
      expectedSHA256: expectedSHA256,
      plannedAtUnix: UInt64(Date().timeIntervalSince1970)
    )
    bootMediaDownloadPlans[id] = metadata
    return metadata
  }

  func downloadBootMedia(
    kind requestedKind: BootMediaStatusEntry.Kind?,
    on id: VirtualMachine.ID
  ) async throws -> BootMediaDownloadResultMetadata {
    try await Task.sleep(nanoseconds: 260_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let plan = bootMediaDownloadPlans[id]
    let kind =
      requestedKind?.isImportable == true
      ? requestedKind ?? .installerImage
      : (plan?.kind
        ?? (vm.guest.localizedCaseInsensitiveContains("macOS")
          ? .macosRestoreImage : .installerImage))
    let url =
      plan?.url
      ?? "https://example.invalid/\(vm.name.lowercased().replacingOccurrences(of: " ", with: "-")).iso"
    let destination =
      plan?.destination
      ?? "installers/\(URL(string: url)?.lastPathComponent ?? "\(vm.name.lowercased().replacingOccurrences(of: " ", with: "-")).iso")"
    let metadata = BootMediaDownloadResultMetadata(
      vm: vm.name,
      kind: kind,
      url: url,
      destination: destination,
      bytes: 5_368_709_120,
      replaced: bootMediaImports[id] != nil,
      expectedSHA256: plan?.expectedSHA256,
      actualSHA256: plan?.expectedSHA256,
      verified: plan?.expectedSHA256 == nil ? nil : true,
      downloaded: true,
      downloadedAtUnix: UInt64(Date().timeIntervalSince1970)
    )
    bootMediaDownloads[id] = metadata
    return metadata
  }

  func inspectLifecyclePlan(action: LifecyclePlanAction, on id: VirtualMachine.ID) async throws
    -> LifecyclePlan
  {
    try await Task.sleep(nanoseconds: 90_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let targetState: VirtualMachine.Status = action == .suspend ? .suspended : .running
    var blockers: [String] = []
    var notes = ["metadata-only lifecycle plan; no backend command was sent"]
    let validTransition =
      (action == .suspend && vm.status == .running)
      || (action == .resume && vm.status == .suspended)

    if !validTransition {
      blockers.append("invalid-lifecycle-transition:\(vm.status.rawValue)->\(targetState.rawValue)")
    }

    let backend: String
    let qmpCommand: String?
    let socketPath: String?
    let socketAvailable: Bool

    switch vm.mode {
    case .compatibility:
      backend = "qemu-qmp"
      qmpCommand = action == .suspend ? "stop" : "cont"
      socketPath = "target/bridgevm-dev/vms/\(vm.name).vmbridge/run/qmp.sock"
      socketAvailable = vm.status == .running || vm.status == .paused || vm.status == .suspended
      if !socketAvailable {
        blockers.append("qmp-socket-unavailable:\(socketPath ?? "")")
      }
      notes.append("Compatibility Mode lifecycle control maps to QMP stop/cont")
    case .fast:
      backend = "apple-vz"
      qmpCommand = nil
      socketPath = nil
      socketAvailable = false
      notes.append(
        "Fast Mode suspend/resume is wired through the runner via Apple VZ saveMachineState/restoreMachineState (not QMP); a real suspend/resume requires a signed AppleVzRunner (BRIDGEVM_APPLE_VZ_RUNNER)"
      )
    }

    return LifecyclePlan(
      vm: vm.name,
      action: action,
      currentState: vm.status,
      targetState: targetState,
      backend: backend,
      metadataOnly: true,
      executable: blockers.isEmpty,
      qmpCommand: qmpCommand,
      socketPath: socketPath,
      socketAvailable: socketAvailable,
      blockers: blockers,
      notes: notes
    )
  }

  func inspectOpenPortPlan(
    guestPort: UInt16,
    scheme: String,
    on id: VirtualMachine.ID
  ) async throws -> OpenPortPlan {
    try await Task.sleep(nanoseconds: 90_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let normalizedScheme =
      scheme.trimmingCharacters(in: .whitespacesAndNewlines).lowercased().isEmpty
      ? "http"
      : scheme.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    let hostPort: UInt16 =
      guestPort == 22 ? 2222 : UInt16(min(UInt32(guestPort) + 10_000, UInt32(UInt16.max)))
    let url = "\(normalizedScheme)://127.0.0.1:\(hostPort)"

    return OpenPortPlan(
      vm: vm.name,
      scheme: normalizedScheme,
      host: "127.0.0.1",
      guestPort: guestPort,
      hostPort: hostPort,
      url: url,
      command: ["open", url]
    )
  }

  func inspectNetworkPlan(on id: VirtualMachine.ID) async throws -> NetworkPlan {
    try await Task.sleep(nanoseconds: 90_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let backend = vm.mode == .fast ? "apple-vz" : "qemu"
    let mode = vm.mode == .fast ? "nat" : "user"
    let forwards = mockPortForwards(for: vm)
    let capabilities = NetworkCapabilities(
      guestOutbound: true,
      hostToGuest: !forwards.isEmpty,
      guestToHost: true,
      hostVisibleHostname: vm.mode == .fast,
      supportsPortForwarding: true,
      requiresPrivilegedHelper: false
    )

    return NetworkPlan(
      vm: vm.name,
      backend: backend,
      mode: mode,
      hostname: vm.name.lowercased().replacingOccurrences(of: " ", with: "-"),
      dryRun: true,
      executable: true,
      portForwards: forwards,
      capabilities: capabilities,
      blockers: [],
      notes: [
        "mock dry-run network plan; no backend launch or host networking mutation was performed"
      ]
    )
  }

  func inspectSSHPlan(user userText: String, on id: VirtualMachine.ID) async throws -> SSHPlan {
    try await Task.sleep(nanoseconds: 90_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let user = userText.trimmingCharacters(in: .whitespacesAndNewlines)
    let sshForward = mockPortForwards(for: vm).filter { $0.guest == 22 }.min { $0.host < $1.host }
    let host = sshForward == nil ? (vm.ipAddress ?? "192.168.64.23") : "127.0.0.1"
    let port = sshForward?.host ?? 22
    var command = ["ssh"]
    if port != 22 {
      command.append(contentsOf: ["-p", "\(port)"])
    }
    command.append("\(user)@\(host)")

    return SSHPlan(
      vm: vm.name,
      user: user,
      host: host,
      port: port,
      source: sshForward == nil ? .guestToolsIP : .portForward,
      command: command
    )
  }

  func listPortForwards(on id: VirtualMachine.ID) async throws -> VMPortForwardList {
    try await Task.sleep(nanoseconds: 90_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    return VMPortForwardList(vm: vm.name, forwards: mockPortForwards(for: vm))
  }

  func addPortForward(host: UInt16, guest: UInt16, on id: VirtualMachine.ID) async throws
    -> VMPortForwardList
  {
    try await Task.sleep(nanoseconds: 120_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    var forwards = mockPortForwards(for: vm)
    forwards.removeAll { $0.host == host || $0.guest == guest }
    forwards.append(VMPortForward(host: host, guest: guest))
    forwards.sort { ($0.host, $0.guest) < ($1.host, $1.guest) }
    portForwardsByID[id] = forwards
    return VMPortForwardList(vm: vm.name, forwards: forwards)
  }

  func removePortForward(host: UInt16, guest: UInt16, on id: VirtualMachine.ID) async throws
    -> VMPortForwardList
  {
    try await Task.sleep(nanoseconds: 100_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    var forwards = mockPortForwards(for: vm)
    forwards.removeAll { $0.host == host && $0.guest == guest }
    portForwardsByID[id] = forwards
    return VMPortForwardList(vm: vm.name, forwards: forwards)
  }

  func inspectGuestToolsStatus(on id: VirtualMachine.ID) async throws -> GuestToolsStatus {
    try await Task.sleep(nanoseconds: 150_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let capabilities = [
      GuestToolsCapability(name: "heartbeat", maxVersion: 1, enabledBy: "base"),
      GuestToolsCapability(name: "guest-ip", maxVersion: 1, enabledBy: "base"),
      GuestToolsCapability(name: "time-sync", maxVersion: 1, enabledBy: "base"),
      GuestToolsCapability(name: "display-resize", maxVersion: 1, enabledBy: "display"),
      GuestToolsCapability(name: "clipboard", maxVersion: 1, enabledBy: "integration.clipboard"),
      GuestToolsCapability(
        name: "shared-folders", maxVersion: 1, enabledBy: "integration.shared_folders"),
      GuestToolsCapability(name: "drag-drop", maxVersion: 1, enabledBy: "integration.dragDrop"),
      GuestToolsCapability(
        name: "applications", maxVersion: 1, enabledBy: "integration.applications"),
      GuestToolsCapability(name: "windows", maxVersion: 1, enabledBy: "integration.windows"),
      GuestToolsCapability(name: "guest-metrics", maxVersion: 1, enabledBy: "diagnostics"),
    ]
    let connected = vm.status == .running || vm.status == .paused
    let now = UInt64(Date().timeIntervalSince1970)
    let sharedFolders = mockSharedFolders(for: vm)
    let runtime =
      connected
      ? GuestToolsRuntime(
        connected: true,
        guestOS: vm.guest,
        agentVersion: "mock-0.1.0",
        capabilities: capabilities.map(\.name),
        lastHeartbeatAtUnix: now,
        guestIPAddresses: vm.ipAddress.map { [GuestToolsIPAddress(address: $0, interface: "en0")] }
          ?? [],
        sharedFolders: sharedFolders.map {
          GuestToolsSharedFolder(
            name: $0.name,
            hostPathToken: $0.hostPathToken,
            mountedAtUnix: mountedSharedFolderNames[id]?.contains($0.name) == true ? now : nil
          )
        },
        metrics: GuestToolsMetrics(
          cpuPercent: vm.status == .running ? 7 : 0,
          memoryUsedMiB: UInt64(vm.resources.memoryGB * 512),
          updatedAtUnix: now
        ),
        updatedAtUnix: now
      )
      : nil

    return GuestToolsStatus(
      vm: vm.name,
      tools: vm.mode == .fast ? "required" : "optional",
      tokenCreatedAtUnix: now > 600 ? now - 600 : 0,
      capabilities: capabilities,
      approvedSharedFolders: sharedFolders.map {
        GuestToolsApprovedSharedFolder(
          name: $0.name,
          hostPath: $0.hostPath,
          hostPathToken: $0.hostPathToken,
          readOnly: $0.readOnly,
          approval: "required"
        )
      },
      runtime: runtime
    )
  }

  func inspectGuestToolsToken(on id: VirtualMachine.ID) async throws -> GuestToolsToken {
    try await Task.sleep(nanoseconds: 60_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let now = UInt64(Date().timeIntervalSince1970)
    return GuestToolsToken(
      vm: vm.name,
      createdAtUnix: now > 600 ? now - 600 : 0,
      tokenLength: 64
    )
  }

  func inspectGuestToolsLinuxCommand(
    transport: GuestToolsLinuxCommandTransport,
    on id: VirtualMachine.ID
  ) async throws -> GuestToolsLinuxCommand {
    try await Task.sleep(nanoseconds: 60_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    return GuestToolsLinuxCommand(
      vm: vm.name,
      transport: transport,
      command: [
        "bridgevm-guest-tools",
        "run",
        "--transport",
        transport.rawValue,
        "--token-file",
        "/run/bridgevm/guest-tools-token.json",
      ],
      tokenFile: "/run/bridgevm/guest-tools-token.json",
      capabilities: ["heartbeat", "time-sync", "guest-ip"]
    )
  }

  func listSharedFolders(on id: VirtualMachine.ID) async throws -> VMSharedFolderList {
    try await Task.sleep(nanoseconds: 90_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    return VMSharedFolderList(vm: vm.name, sharedFolders: mockSharedFolders(for: vm))
  }

  func addSharedFolder(
    named shareName: String,
    hostPath: String,
    readOnly: Bool,
    hostPathToken: String?,
    on id: VirtualMachine.ID
  ) async throws -> VMSharedFolderList {
    try await Task.sleep(nanoseconds: 140_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    var shares = mockSharedFolders(for: vm)
    shares.removeAll { $0.name == shareName }
    shares.append(
      VMSharedFolder(
        name: shareName,
        hostPath: hostPath,
        readOnly: readOnly,
        hostPathToken: hostPathToken ?? Self.stableShareToken(name: shareName, hostPath: hostPath)
      ))
    sharedFoldersByID[id] = shares
    return VMSharedFolderList(vm: vm.name, sharedFolders: shares)
  }

  func removeSharedFolder(named shareName: String, on id: VirtualMachine.ID) async throws
    -> VMSharedFolderList
  {
    try await Task.sleep(nanoseconds: 120_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    var shares = mockSharedFolders(for: vm)
    shares.removeAll { $0.name == shareName }
    sharedFoldersByID[id] = shares
    mountedSharedFolderNames[id]?.remove(shareName)
    return VMSharedFolderList(vm: vm.name, sharedFolders: shares)
  }

  func mountApprovedSharedFolder(named shareName: String, on id: VirtualMachine.ID) async throws
    -> GuestToolsStatus?
  {
    try await Task.sleep(nanoseconds: 180_000_000)

    guard virtualMachines.contains(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    mountedSharedFolderNames[id, default: []].insert(shareName)
    return try await inspectGuestToolsStatus(on: id)
  }

  func unmountApprovedSharedFolder(named shareName: String, on id: VirtualMachine.ID) async throws
    -> GuestToolsStatus?
  {
    try await Task.sleep(nanoseconds: 160_000_000)

    guard virtualMachines.contains(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    mountedSharedFolderNames[id]?.remove(shareName)
    return try await inspectGuestToolsStatus(on: id)
  }

  func sendGuestToolsCommand(
    _ command: GuestToolsAgentCommand,
    requestID: String?,
    on id: VirtualMachine.ID
  ) async throws -> GuestToolsCommandDispatch {
    try await Task.sleep(nanoseconds: 120_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    return GuestToolsCommandDispatch(
      vm: vm.name,
      requestID: requestID,
      pendingCommands: requestID == nil ? 0 : 1
    )
  }

  func inspectQMPStatus(on id: VirtualMachine.ID) async throws -> QMPStatus {
    try await Task.sleep(nanoseconds: 90_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let available = vm.status == .running || vm.status == .paused
    return QMPStatus(
      socketPath: "target/bridgevm-dev/vms/\(vm.name).vmbridge/run/qmp.sock",
      available: available,
      status: available ? (vm.status == .paused ? "paused" : "running") : nil,
      running: available ? vm.status == .running : nil
    )
  }

  func viewLogs(kind: VMLogKind, bytes: UInt64?, on id: VirtualMachine.ID) async throws
    -> VMLogView
  {
    try await Task.sleep(nanoseconds: 90_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let content = """
      \(kind.title) log tail for \(vm.name)
      backend: \(vm.mode.title)
      status: \(vm.status.title)
      """
    let data = Data(content.utf8)
    let limit = Int(min(bytes ?? 8 * 1024, UInt64(data.count)))
    let suffix = String(decoding: data.suffix(limit), as: UTF8.self)
    return VMLogView(
      vm: vm.name,
      kind: kind,
      path:
        "target/bridgevm-dev/vms/\(vm.name).vmbridge/logs/\(kind == .qemu ? "qemu.log" : "serial.log")",
      exists: true,
      bytes: UInt64(data.count),
      returnedBytes: UInt64(limit),
      truncated: limit < data.count,
      content: suffix
    )
  }

  func inspectRunnerStatus(on id: VirtualMachine.ID) async throws -> RunnerStatus? {
    try await Task.sleep(nanoseconds: 120_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let now = UInt64(Date().timeIntervalSince1970)
    let readiness =
      vm.mode == .fast
      ? LaunchReadiness(
        ready: vm.status == .running,
        blockers: vm.status == .running
          ? []
          : [
            LaunchReadinessBlocker(
              code: "missing-primary-disk",
              message:
                "Primary disk is missing; prepare or create the disk before Fast Mode launch.",
              path: "disks/root.qcow2"
            )
          ]
      )
      : nil

    return RunnerStatus(
      engine: vm.mode == .fast ? "lightvm" : "fullvm",
      pid: vm.status == .running ? 42 : nil,
      command: vm.mode == .fast
        ? ["lightvm-runner", vm.name, "--apple-vz"]
        : ["qemu-system-aarch64", "-name", vm.name],
      logPath: vm.mode == .fast ? "logs/lightvm.log" : "logs/qemu.log",
      startedAtUnix: vm.status == .running && now > 900 ? now - 900 : now,
      dryRun: vm.status != .running,
      launchReadiness: readiness
    )
  }

  func inspectQemuArgs(on id: VirtualMachine.ID) async throws -> QemuLaunchPlan {
    try await Task.sleep(nanoseconds: 90_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let networkArgs =
      vm.mode == .compatibility
      ? ["-netdev", "vmnet-host,id=net0", "-device", "virtio-net-pci,netdev=net0"]
      : ["-netdev", "user,id=net0,hostfwd=tcp::2222-:22", "-device", "virtio-net-pci,netdev=net0"]

    return QemuLaunchPlan(
      program: "qemu-system-aarch64",
      args: [
        "-name",
        vm.name,
        "-machine",
        "virt",
        "-m",
        "\(vm.resources.memoryGB)G",
        "-smp",
        "\(vm.resources.cpuCount)",
      ] + networkArgs
    )
  }

  func prepareRun(on id: VirtualMachine.ID) async throws -> RunnerStatus {
    guard let status = try await inspectRunnerStatus(on: id) else {
      throw VirtualMachineClientError.daemonResponseInvalid
    }
    return status
  }

  func exportVirtualMachine(on id: VirtualMachine.ID, output: String) async throws
    -> VMExportMetadata
  {
    try await Task.sleep(nanoseconds: 140_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    return VMExportMetadata(
      vm: vm.name,
      source: "target/bridgevm-dev/vms/\(vm.name).vmbridge",
      output: output,
      archiveFormat: "directory",
      copiedFileCount: 3,
      copiedFiles: [
        "manifest.yaml",
        "metadata/state.json",
        "metadata/runtime.json",
      ],
      manifestPreserved: true,
      metadataPreserved: true,
      exportedAtUnix: UInt64(Date().timeIntervalSince1970)
    )
  }

  func importVirtualMachine(input: String, name: String?) async throws -> VMImportMetadata {
    try await Task.sleep(nanoseconds: 160_000_000)

    let trimmedName = name?.trimmingCharacters(in: .whitespacesAndNewlines)
    let requestedName = trimmedName?.isEmpty == false ? trimmedName : nil
    let originalName = URL(fileURLWithPath: input)
      .deletingPathExtension()
      .lastPathComponent
      .replacingOccurrences(of: ".vmbridge", with: "")
    let importedName = requestedName ?? originalName
    let vmName = importedName.isEmpty ? "Imported VM" : importedName
    let imported = VirtualMachine(
      id: UUID.bridgeVMStableNameID(vmName),
      name: vmName,
      guest: "Imported Guest",
      status: .stopped,
      mode: .compatibility,
      resources: .init(cpuCount: 2, memoryGB: 4, diskGB: 40),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "Imported from \(input)."
    )
    virtualMachines.removeAll { $0.name == imported.name }
    virtualMachines.append(imported)
    return VMImportMetadata(
      vm: imported.name,
      source: input,
      output: "target/bridgevm-dev/vms/\(imported.name).vmbridge",
      archiveFormat: "directory",
      copiedFileCount: 3,
      copiedFiles: [
        "manifest.yaml",
        "metadata/state.json",
        "metadata/runtime.json",
      ],
      manifestPreserved: true,
      metadataPreserved: true,
      originalName: originalName,
      requestedName: requestedName,
      manifestIdentityRewritten: requestedName != nil,
      importedAtUnix: UInt64(Date().timeIntervalSince1970)
    )
  }

  func inspectSnapshotPreflightStatus(on id: VirtualMachine.ID) async throws
    -> SnapshotPreflightStatus
  {
    try await Task.sleep(nanoseconds: 120_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let guestToolsConnected = vm.status == .running || vm.status == .paused
    var blockers: [SnapshotPreflightBlocker] = []
    if !guestToolsConnected {
      blockers.append(
        SnapshotPreflightBlocker(
          code: "guest-tools-not-connected",
          message:
            "Guest tools must be connected before application-consistent preflight can pass.",
          path: nil
        ))
    }

    return SnapshotPreflightStatus(
      vm: vm.name,
      consistency: .applicationConsistent,
      backendFreezeThawSupported: true,
      guestToolsConnected: guestToolsConnected,
      capabilities: guestToolsConnected ? ["guest-tools-heartbeat", "fs-freeze", "fs-thaw"] : [],
      ready: guestToolsConnected,
      blockers: blockers,
      checkedAtUnix: UInt64(Date().timeIntervalSince1970)
    )
  }

  func listSnapshots(on id: VirtualMachine.ID) async throws -> [VMSnapshot] {
    try await Task.sleep(nanoseconds: 80_000_000)

    guard virtualMachines.contains(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    return [
      VMSnapshot(
        name: "before-upgrade",
        kind: .disk,
        createdAtUnix: UInt64(Date().timeIntervalSince1970) - 3600,
        vmState: .stopped
      ),
      VMSnapshot(
        name: "paused-state",
        kind: .suspend,
        createdAtUnix: UInt64(Date().timeIntervalSince1970) - 900,
        vmState: .suspended
      ),
    ] + (snapshotsByID[id] ?? [])
  }

  func createSnapshot(named snapshotName: String, kind: VMSnapshotKind, on id: VirtualMachine.ID)
    async throws -> VMSnapshot
  {
    try await Task.sleep(nanoseconds: 120_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let snapshot = VMSnapshot(
      name: snapshotName,
      kind: kind,
      createdAtUnix: UInt64(Date().timeIntervalSince1970),
      vmState: vm.status
    )
    snapshotsByID[id, default: []].removeAll { $0.name == snapshotName }
    snapshotsByID[id, default: []].append(snapshot)
    return snapshot
  }

  func restoreSnapshot(named snapshotName: String, on id: VirtualMachine.ID) async throws
    -> SnapshotRestoreResult
  {
    try await Task.sleep(nanoseconds: 90_000_000)

    guard virtualMachines.contains(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    return SnapshotRestoreResult(
      snapshot: snapshotName,
      restoredAtUnix: UInt64(Date().timeIntervalSince1970),
      restoredState: snapshotName.localizedCaseInsensitiveContains("paused")
        ? .suspended : .stopped,
      activeDisk: SnapshotActiveDisk(
        source: "snapshot-backing",
        snapshot: snapshotName,
        path: "disks/root.qcow2",
        format: "qcow2",
        exists: true,
        activatedAtUnix: UInt64(Date().timeIntervalSince1970)
      ),
      suspendImage: snapshotName.localizedCaseInsensitiveContains("paused")
        ? SnapshotSuspendImage(
          snapshot: snapshotName,
          imagePath: "suspend-images/\(snapshotName).bin",
          imageFormat: "bridgevm-suspend-v1",
          imageExists: true,
          preparedAtUnix: UInt64(Date().timeIntervalSince1970) - 900
        )
        : nil
    )
  }

  func inspectSnapshotChain(on id: VirtualMachine.ID) async throws -> VMSnapshotChain {
    try await Task.sleep(nanoseconds: 80_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let disks = snapshotDisksByID[id] ?? []
    let activeDisk =
      disks.first.map {
        VMActiveDisk(
          source: "snapshot-overlay",
          snapshot: $0.snapshot,
          path: $0.overlayPath,
          format: $0.overlayFormat,
          exists: $0.overlayExists,
          activatedAtUnix: $0.preparedAtUnix
        )
      }
      ?? VMActiveDisk(
        source: "primary",
        snapshot: nil,
        path: "target/bridgevm-dev/vms/\(vm.name).vmbridge/disks/root.qcow2",
        format: "qcow2",
        exists: true,
        activatedAtUnix: UInt64(Date().timeIntervalSince1970)
      )

    return VMSnapshotChain(activeDisk: activeDisk, disks: disks)
  }

  func createSnapshotDisk(named snapshotName: String, on id: VirtualMachine.ID) async throws
    -> VMSnapshotDiskCreation
  {
    try await Task.sleep(nanoseconds: 120_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let bundle = "target/bridgevm-dev/vms/\(vm.name).vmbridge"
    let backingPath = "\(bundle)/disks/root.qcow2"
    let overlayPath = "\(bundle)/disks/snapshots/\(snapshotName).qcow2"
    let command = [
      "qemu-img", "create", "-f", "qcow2", "-F", "qcow2", "-b", backingPath, overlayPath,
    ]
    let now = UInt64(Date().timeIntervalSince1970)
    let disk = VMSnapshotDisk(
      snapshot: snapshotName,
      overlayPath: overlayPath,
      overlayFormat: "qcow2",
      overlayExists: true,
      backingPath: backingPath,
      backingFormat: "qcow2",
      backingExists: true,
      createCommand: command,
      preparedAtUnix: now
    )
    snapshotDisksByID[id, default: []].removeAll { $0.snapshot == snapshotName }
    snapshotDisksByID[id, default: []].append(disk)

    return VMSnapshotDiskCreation(
      snapshot: snapshotName,
      disk: disk,
      command: command,
      executed: true,
      exitStatus: "exit status: 0",
      stdout: "created overlay\n",
      stderr: "",
      createdAtUnix: now
    )
  }

  func preparePrimaryDisk(on id: VirtualMachine.ID) async throws -> DiskPreparation {
    try await Task.sleep(nanoseconds: 80_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let preparation = mockDiskPreparation(for: vm, existing: diskPreparationsByID[id]?.exists)
    diskPreparationsByID[id] = preparation
    return preparation
  }

  func createPrimaryDisk(on id: VirtualMachine.ID) async throws -> VMDiskCreation {
    try await Task.sleep(nanoseconds: 120_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let preparation = mockDiskPreparation(for: vm, existing: true)
    diskPreparationsByID[id] = preparation
    return VMDiskCreation(
      preparation: preparation,
      command: preparation.createCommand,
      executed: true,
      exitStatus: "0",
      stdout:
        "Formatting '\(preparation.path)', fmt=\(preparation.format) size=\(preparation.size)\n",
      stderr: "",
      createdAtUnix: UInt64(Date().timeIntervalSince1970)
    )
  }

  func inspectPrimaryDisk(on id: VirtualMachine.ID) async throws -> VMDiskInspection {
    try await Task.sleep(nanoseconds: 100_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let preparation = diskPreparationsByID[id] ?? mockDiskPreparation(for: vm, existing: true)
    diskPreparationsByID[id] = preparation
    let infoValue = DiskMetadataValue.object([
      "filename": .string(preparation.path),
      "format": .string(preparation.format),
      "virtual-size": .int(
        Int64(preparation.sizeBytes ?? UInt64(vm.resources.diskGB) * 1_073_741_824)),
      "dirty-flag": .bool(false),
    ])

    return VMDiskInspection(
      preparation: preparation,
      command: ["qemu-img", "info", "--output=json", preparation.path],
      exitStatus: "0",
      info: infoValue.prettyPrinted,
      infoValue: infoValue,
      stdout: infoValue.prettyPrinted,
      stderr: "",
      inspectDurationMicroseconds: 18_000,
      inspectedAtUnix: UInt64(Date().timeIntervalSince1970)
    )
  }

  func verifyActiveDisk(on id: VirtualMachine.ID) async throws -> VMDiskVerification {
    try await Task.sleep(nanoseconds: 100_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let activeDisk = mockActiveDisk(for: vm)
    let reportValue = DiskMetadataValue.object([
      "check-errors": .int(0),
      "corruptions": .int(0),
      "image-end-offset": .int(Int64(UInt64(vm.resources.diskGB) * 1_073_741_824)),
      "leaks": .int(0),
    ])

    return VMDiskVerification(
      activeDisk: activeDisk,
      command: ["qemu-img", "check", "--output=json", activeDisk.path],
      exitStatus: "exit status: 0",
      report: reportValue.prettyPrinted,
      reportValue: reportValue,
      stdout: reportValue.prettyPrinted,
      stderr: "",
      verifyDurationMicroseconds: 24_000,
      verifiedAtUnix: UInt64(Date().timeIntervalSince1970)
    )
  }

  func compactActiveDisk(on id: VirtualMachine.ID) async throws -> VMDiskCompaction {
    try await Task.sleep(nanoseconds: 140_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let preparation = diskPreparationsByID[id] ?? mockDiskPreparation(for: vm, existing: true)
    diskPreparationsByID[id] = preparation
    let activeDisk = mockActiveDisk(for: vm)
    let timestamp = UInt64(Date().timeIntervalSince1970)
    let originalSize = preparation.sizeBytes ?? UInt64(vm.resources.diskGB) * 1_073_741_824
    let compactedSize = originalSize > 1 ? originalSize * 4 / 5 : originalSize
    let tempPath = activeDisk.path.replacingOccurrences(of: ".qcow2", with: ".compact.tmp.qcow2")
    let backupPath = activeDisk.path.replacingOccurrences(
      of: ".qcow2",
      with: ".precompact-\(timestamp).qcow2"
    )

    return VMDiskCompaction(
      preparation: preparation,
      activeDisk: activeDisk,
      command: [
        "qemu-img", "convert", "-O", activeDisk.format, activeDisk.path, tempPath,
      ],
      tempPath: tempPath,
      backupPath: backupPath,
      exitStatus: "exit status: 0",
      stdout: "mock compacted \(activeDisk.path)\n",
      stderr: "",
      originalSizeBytes: originalSize,
      compactedSizeBytes: compactedSize,
      compactDurationMicroseconds: 58_000,
      compactedAtUnix: timestamp
    )
  }

  func repairMetadata(on id: VirtualMachine.ID) async throws -> VMMetadataRepair {
    try await Task.sleep(nanoseconds: 90_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let repaired = vm.status == .stopped || vm.mode == .compatibility
    return VMMetadataRepair(
      vm: vm.name,
      bundle: "target/bridgevm-dev/vms/\(vm.name).vmbridge",
      repaired: repaired,
      actions: repaired
        ? [
          VMMetadataRepairAction(
            action: "repaired",
            path: "metadata/active-disk.json",
            detail: "Recreated repairable active disk metadata from the manifest."
          )
        ] : [],
      repairedAtUnix: UInt64(Date().timeIntervalSince1970)
    )
  }

  func migrateManifest(on id: VirtualMachine.ID, dryRun: Bool) async throws -> VMManifestMigration {
    try await Task.sleep(nanoseconds: 90_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let bundle = "target/bridgevm-dev/vms/\(vm.name).vmbridge"
    return VMManifestMigration(
      vm: vm.name,
      bundle: bundle,
      manifestPath: "\(bundle)/manifest.yaml",
      dryRun: dryRun,
      migrated: false,
      fromSchema: "bridgevm.io/v1",
      toSchema: "bridgevm.io/v1",
      actions: dryRun
        ? [
          "validated current manifest schema",
          "dry-run did not write migration receipt or manifest backup",
        ]
        : [
          "validated current manifest schema", "copied manifest before migration",
          "wrote migration receipt",
        ],
      backupPath: dryRun ? nil : "\(bundle)/metadata/manifest-before-migration.yaml",
      receiptPath: dryRun ? nil : "\(bundle)/metadata/manifest-migration.json",
      migratedAtUnix: UInt64(Date().timeIntervalSince1970)
    )
  }

  func executeApplicationConsistentSnapshot(
    named snapshotName: String,
    freezeTimeoutMillis: UInt64?,
    on id: VirtualMachine.ID
  ) async throws -> ApplicationConsistentSnapshotExecution {
    try await Task.sleep(nanoseconds: 220_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let freezeRequestID = "application-consistent-snapshot:\(snapshotName):freeze"
    let thawRequestID = "application-consistent-snapshot:\(snapshotName):thaw"
    let now = UInt64(Date().timeIntervalSince1970)
    return ApplicationConsistentSnapshotExecution(
      vm: vm.name,
      snapshot: snapshotName,
      freezeRequestID: freezeRequestID,
      thawRequestID: thawRequestID,
      pendingCommandsAfterFreeze: 1,
      pendingCommandsAfterThaw: 2,
      snapshotCreatedAtUnix: now,
      freezeResult: ApplicationConsistentSnapshotCommandResult(
        requestID: freezeRequestID,
        capability: "fs-freeze",
        ok: true,
        errorCode: nil,
        message: "freeze scaffold acknowledged",
        completedAtUnix: now
      ),
      thawResult: ApplicationConsistentSnapshotCommandResult(
        requestID: thawRequestID,
        capability: "fs-thaw",
        ok: true,
        errorCode: nil,
        message: "thaw scaffold acknowledged",
        completedAtUnix: now
      ),
      preflightReady: true,
      note:
        "Mock guest-tools freeze/thaw scaffold completed around snapshot metadata creation; this is not OS-level application consistency."
    )
  }

  func reapplyRuntimeResources(
    visibility: RuntimeResourceVisibility,
    on id: VirtualMachine.ID
  ) async throws -> RuntimeResourcePolicy {
    try await Task.sleep(nanoseconds: 120_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let isBackground = visibility == .background
    let memory = isBackground ? max(2, vm.resources.memoryGB / 2) : vm.resources.memoryGB
    let cpu = isBackground ? max(1, vm.resources.cpuCount / 2) : vm.resources.cpuCount
    let policy = RuntimeResourcePolicy(
      vm: vm.name,
      mode: vm.mode.rawValue,
      profile: "automatic",
      visibility: visibility,
      state: vm.status.rawValue,
      onBattery: false,
      memory: "\(memory * 1024)",
      cpu: "\(cpu)",
      displayFPSCap: isBackground ? "10" : "60",
      rationale: isBackground
        ? "Mock background policy records lower CPU, memory, and display pacing."
        : "Mock foreground policy records full configured resources.",
      liveApplied: false,
      liveApplyBlockers: [
        RuntimeResourcePolicyBlocker(
          code: "mock-runtime-control-unavailable",
          message: "Mock inventory records the policy but does not control a live backend."
        )
      ],
      updatedAtUnix: UInt64(Date().timeIntervalSince1970)
    )
    runtimeResourcePoliciesByID[id] = policy
    return policy
  }

  func createDiagnosticBundle(output: String?, on id: VirtualMachine.ID) async throws
    -> DiagnosticBundle
  {
    try await Task.sleep(nanoseconds: 120_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let outputRoot = mockOutputRoot(output, fallback: "target/bridgevm-dev/diagnostics")
    let logFile = vm.mode == .fast ? "logs/lightvm.log" : "logs/qemu.log"
    return DiagnosticBundle(
      vm: vm.name,
      source: mockBundlePath(for: vm),
      output: "\(outputRoot)/\(mockArtifactSlug(for: vm))-diagnostics",
      files: [
        "manifest.yaml",
        "metadata/state.json",
        "metadata/runtime.json",
        logFile,
      ],
      createdAtUnix: UInt64(Date().timeIntervalSince1970)
    )
  }

  func createPerformanceBaseline(output: String?, on id: VirtualMachine.ID) async throws
    -> PerformanceBaseline
  {
    try await Task.sleep(nanoseconds: 120_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let outputRoot = mockOutputRoot(output, fallback: "target/bridgevm-dev/performance")
    let guestTools = try await inspectGuestToolsStatus(on: id)
    let metrics = guestTools.runtime?.metrics
    let connected = guestTools.connected ? UInt64(1) : UInt64(0)
    return PerformanceBaseline(
      vm: vm.name,
      source: mockBundlePath(for: vm),
      output: outputRoot,
      artifact: "\(outputRoot)/performance-baseline.json",
      createdAtUnix: UInt64(Date().timeIntervalSince1970),
      metadataOnly: true,
      state: vm.status,
      runner: try await inspectRunnerStatus(on: id),
      guestTools: guestTools,
      metrics: metrics,
      measurements: [
        PerformanceMeasurement(
          name: "guest_tools_connected",
          value: connected,
          unit: "bool",
          source: "metadata.guest_tools",
          metadataOnly: true
        ),
        PerformanceMeasurement(
          name: "guest_benchmark_cpu_iterations",
          value: connected == 1 ? 250_000 : 0,
          unit: "iterations",
          source: "guest_tools.benchmark.cpu.iterations",
          metadataOnly: true
        ),
        PerformanceMeasurement(
          name: "guest_benchmark_disk_bytes_written",
          value: connected == 1 ? 1_048_576 : 0,
          unit: "bytes",
          source: "guest_tools.benchmark.disk.bytes_written",
          metadataOnly: true
        ),
      ],
      notes: [
        "mock baseline includes guest benchmark-shaped measurements for UI coverage",
        "use bridgevmd for live guest benchmark evidence",
      ]
    )
  }

  func createPerformanceSample(
    output: String?,
    artifactBytes: UInt64,
    iterations: UInt16,
    sync: Bool,
    on id: VirtualMachine.ID
  ) async throws -> PerformanceSample {
    try await Task.sleep(nanoseconds: 140_000_000)

    guard let vm = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let outputRoot = mockOutputRoot(output, fallback: "target/bridgevm-dev/performance")
    let effectiveBytes = max(UInt64(1), artifactBytes)
    let effectiveIterations = max(UInt16(1), iterations)
    let probes = (1...Int(effectiveIterations)).map { "\(outputRoot)/probe-\($0).bin" }
    let iterationResults = (1...Int(effectiveIterations)).map { index in
      PerformanceSampleIteration(
        iteration: UInt16(index),
        probe: "\(outputRoot)/probe-\(index).bin",
        bytes: effectiveBytes,
        writeLatencyMicroseconds: 120 + UInt64(index * 8) + (sync ? 40 : 0),
        sync: sync
      )
    }
    let totalBytes = effectiveBytes.multipliedReportingOverflow(by: UInt64(effectiveIterations))
    let guestTools = try await inspectGuestToolsStatus(on: id)
    let metrics = guestTools.runtime?.metrics
    let benchmarkConnected = guestTools.connected ? UInt64(1) : UInt64(0)

    return PerformanceSample(
      vm: vm.name,
      source: mockBundlePath(for: vm),
      output: outputRoot,
      artifact: "\(outputRoot)/performance-sample.json",
      probe: probes[0],
      probes: probes,
      artifactBytes: effectiveBytes,
      iterations: effectiveIterations,
      sync: sync,
      iterationResults: iterationResults,
      createdAtUnix: UInt64(Date().timeIntervalSince1970),
      state: vm.status,
      runner: try await inspectRunnerStatus(on: id),
      guestTools: guestTools,
      metrics: metrics,
      measurements: [
        PerformanceMeasurement(
          name: "host_artifact_write_total_bytes",
          value: totalBytes.overflow ? UInt64.max : totalBytes.partialValue,
          unit: "bytes",
          source: "host.fs.write_probe",
          metadataOnly: false
        ),
        PerformanceMeasurement(
          name: "guest_benchmark_cpu_ops_per_sec",
          value: benchmarkConnected == 1 ? 75_000 : 0,
          unit: "ops/s",
          source: "guest_tools.benchmark.cpu.ops_per_sec",
          metadataOnly: true
        ),
        PerformanceMeasurement(
          name: "guest_benchmark_disk_mib_per_sec",
          value: benchmarkConnected == 1 ? 96 : 0,
          unit: "MiB/s",
          source: "guest_tools.benchmark.disk.mib_per_sec",
          metadataOnly: true
        ),
      ],
      notes: [
        "mock sample records host-side probe metadata",
        "guest benchmark measurements are mock fixtures; use bridgevmd for live evidence",
      ]
    )
  }

  func createVirtualMachine(_ request: CreateVirtualMachineRequest) async throws -> VirtualMachine {
    try await Task.sleep(nanoseconds: 260_000_000)

    let name = request.name.trimmingCharacters(in: .whitespacesAndNewlines)
    let virtualMachine = VirtualMachine(
      id: UUID(),
      name: name,
      guest: request.template.guestTitle,
      status: .stopped,
      mode: request.template.engineMode,
      resources: .init(cpuCount: 0, memoryGB: 0, diskGB: 80),
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: "Created from \(request.template.mediaLabel)."
    )
    virtualMachines.insert(virtualMachine, at: 0)
    return virtualMachine
  }

  func cloneVirtualMachine(on id: VirtualMachine.ID, newName: String, linked: Bool) async throws
    -> CloneVirtualMachineMetadata
  {
    try await Task.sleep(nanoseconds: 260_000_000)

    guard let source = virtualMachines.first(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let name = newName.trimmingCharacters(in: .whitespacesAndNewlines)
    let cloned = VirtualMachine(
      id: UUID(),
      name: name,
      guest: source.guest,
      status: .stopped,
      mode: source.mode,
      resources: source.resources,
      uptime: "Not running",
      ipAddress: nil,
      lastStarted: nil,
      notes: linked ? "Linked clone of \(source.name)." : "Cloned from \(source.name)."
    )
    virtualMachines.insert(cloned, at: 0)
    let backingPath = "/Mock/\(source.name).vmbridge/disks/root.qcow2"
    let outputPath = "/Mock/\(cloned.name).vmbridge"
    let overlayPath = "\(outputPath)/disks/root.qcow2"
    return CloneVirtualMachineMetadata(
      vm: cloned.name,
      source: "/Mock/\(source.name).vmbridge",
      output: outputPath,
      linked: linked,
      backingPath: linked ? backingPath : nil,
      backingFormat: linked ? "qcow2" : nil,
      createCommand: linked
        ? ["qemu-img", "create", "-f", "qcow2", "-F", "qcow2", "-b", backingPath, overlayPath]
        : nil
    )
  }

  func deleteVirtualMachine(on id: VirtualMachine.ID) async throws -> VMDeletionMetadata {
    try await Task.sleep(nanoseconds: 140_000_000)

    guard let index = virtualMachines.firstIndex(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    let vm = virtualMachines[index]
    guard vm.status == .stopped else {
      throw VirtualMachineClientError.daemonResponseInvalid
    }

    virtualMachines.remove(at: index)
    bootMediaImports[id] = nil
    bootMediaVerifications[id] = nil
    bootMediaDownloadPlans[id] = nil
    bootMediaDownloads[id] = nil
    portForwardsByID[id] = nil
    sharedFoldersByID[id] = nil
    mountedSharedFolderNames[id] = nil
    diskPreparationsByID[id] = nil
    snapshotsByID[id] = nil
    snapshotDisksByID[id] = nil
    runtimeResourcePoliciesByID[id] = nil

    return VMDeletionMetadata(
      path: mockBundlePath(for: vm),
      metadataOnly: true,
      vm: vm.name
    )
  }

  func perform(_ action: VirtualMachineAction, on id: VirtualMachine.ID) async throws
    -> VMActionResult
  {
    try await Task.sleep(nanoseconds: 220_000_000)

    guard let index = virtualMachines.firstIndex(where: { $0.id == id }) else {
      throw VirtualMachineClientError.virtualMachineNotFound
    }

    var vm = virtualMachines[index]
    let message: String

    switch action {
    case .start:
      vm.status = .running
      vm.uptime = "Metadata start recorded"
      vm.lastStarted = Date()
      vm.ipAddress = vm.ipAddress ?? "192.168.64.\(Int.random(in: 30...80))"
      message = "\(vm.name) metadata start recorded."
    case .pause:
      vm.status = .suspended
      vm.uptime = "Suspended"
      message = "\(vm.name) suspended."
    case .resume:
      vm.status = .running
      vm.uptime = "Metadata resume recorded"
      message = "\(vm.name) metadata resume recorded."
    case .stop:
      vm.status = .stopped
      vm.uptime = "Not running"
      vm.ipAddress = nil
      message = "\(vm.name) stopped."
    case .restart:
      vm.status = .running
      vm.uptime = "Metadata restart recorded"
      vm.lastStarted = Date()
      message = "\(vm.name) metadata restart recorded."
    }

    virtualMachines[index] = vm
    return VMActionResult(virtualMachine: vm, message: message)
  }

  private func mockSharedFolders(for vm: VirtualMachine) -> [VMSharedFolder] {
    sharedFoldersByID[vm.id]
      ?? [
        VMSharedFolder(
          name: "workspace",
          hostPath: "~/Projects",
          readOnly: false,
          hostPathToken: "mock-workspace-token"
        )
      ]
  }

  private func mockPortForwards(for vm: VirtualMachine) -> [VMPortForward] {
    portForwardsByID[vm.id]
      ?? [
        VMPortForward(
          host: vm.mode == .compatibility ? 2222 : 18080, guest: vm.mode == .compatibility ? 22 : 80
        )
      ]
  }

  private func mockDiskPreparation(for vm: VirtualMachine, existing: Bool?) -> DiskPreparation {
    let path = "\(mockBundlePath(for: vm))/disks/root.qcow2"
    let sizeBytes = UInt64(vm.resources.diskGB) * 1_073_741_824
    let exists = existing ?? (vm.status != .error)
    return DiskPreparation(
      path: path,
      format: "qcow2",
      size: "\(vm.resources.diskGB)G",
      sizeBytes: sizeBytes,
      exists: exists,
      created: !exists,
      createCommand: ["qemu-img", "create", "-f", "qcow2", path, "\(vm.resources.diskGB)G"],
      preparedAtUnix: UInt64(Date().timeIntervalSince1970)
    )
  }

  private func mockBundlePath(for vm: VirtualMachine) -> String {
    "target/bridgevm-dev/vms/\(vm.name).vmbridge"
  }

  private func mockActiveDisk(for vm: VirtualMachine) -> VMActiveDisk {
    if let disk = snapshotDisksByID[vm.id]?.first {
      return VMActiveDisk(
        source: "snapshot-overlay",
        snapshot: disk.snapshot,
        path: disk.overlayPath,
        format: disk.overlayFormat,
        exists: disk.overlayExists,
        activatedAtUnix: disk.preparedAtUnix
      )
    }

    return VMActiveDisk(
      source: "primary",
      snapshot: nil,
      path: "\(mockBundlePath(for: vm))/disks/root.qcow2",
      format: "qcow2",
      exists: true,
      activatedAtUnix: UInt64(Date().timeIntervalSince1970)
    )
  }

  private func mockOutputRoot(_ output: String?, fallback: String) -> String {
    let trimmed = output?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    return trimmed.isEmpty ? fallback : trimmed
  }

  private func mockArtifactSlug(for vm: VirtualMachine) -> String {
    vm.name
      .lowercased()
      .components(separatedBy: CharacterSet.alphanumerics.inverted)
      .filter { !$0.isEmpty }
      .joined(separator: "-")
  }

  private static func stableShareToken(name: String, hostPath: String) -> String {
    var hash: UInt64 = 0xcbf2_9ce4_8422_2325
    for byte in Array(name.utf8) + [0] + Array(hostPath.utf8) {
      hash ^= UInt64(byte)
      hash &*= 0x100_0000_01b3
    }
    return "share-\(String(hash, radix: 16))"
  }
}
