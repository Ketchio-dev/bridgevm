import Foundation
import SwiftUI

struct VirtualMachine: Identifiable, Equatable {
  enum Status: String, CaseIterable, Equatable {
    case running
    case paused
    case stopped
    case suspended
    case error

    var title: String {
      switch self {
      case .running: return "Running"
      case .paused: return "Paused"
      case .stopped: return "Stopped"
      case .suspended: return "Suspended"
      case .error: return "Needs Attention"
      }
    }

    var tint: Color {
      switch self {
      case .running: return .green
      case .paused: return .yellow
      case .stopped: return .secondary
      case .suspended: return .blue
      case .error: return .red
      }
    }
  }

  enum EngineMode: String, CaseIterable, Codable, Equatable {
    case fast
    case compatibility

    var title: String {
      switch self {
      case .fast: return "Fast Mode"
      case .compatibility: return "Compatibility Mode"
      }
    }
  }

  struct Resources: Equatable {
    var cpuCount: Int
    var memoryGB: Int
    var diskGB: Int
  }

  let id: UUID
  var name: String
  var guest: String
  var status: Status
  var mode: EngineMode
  var resources: Resources
  var uptime: String
  var ipAddress: String?
  var lastStarted: Date?
  var notes: String

  var primaryActionTitle: String {
    switch status {
    case .running: return "Suspend"
    case .paused, .suspended: return "Resume"
    case .stopped, .error: return "Start"
    }
  }

  var canOpenConsole: Bool {
    status == .running || status == .paused
  }
}

struct VMActionResult: Equatable {
  var virtualMachine: VirtualMachine
  var message: String
}

struct VMReadinessSummary: Equatable {
  enum Action: Equatable {
    case refreshBootMedia
    case prepareDisk
    case prepareRun
    case refreshRunner
    case openConsole
    case primaryAction
  }

  enum Severity: Equatable {
    case ready
    case attention
    case blocked
    case informational
  }

  var title: String
  var detail: String
  var actionTitle: String
  var action: Action
  var severity: Severity

  static func evaluate(
    virtualMachine: VirtualMachine,
    bootMediaStatus: BootMediaStatus?,
    bootMediaStatusError: String?,
    runnerStatus: RunnerStatus?,
    runnerStatusError: String?,
    preRunLaunchReadiness: LaunchReadiness? = nil,
    snapshotChain: VMSnapshotChain?,
    snapshotChainError: String?,
    diskPreparation: DiskPreparation?,
    diskCreation: VMDiskCreation?,
    diskInspection: VMDiskInspection?,
    diskVerification: VMDiskVerification?
  ) -> VMReadinessSummary {
    if virtualMachine.canOpenConsole {
      return VMReadinessSummary(
        title: "Console diagnostics available",
        detail: "Probe QMP and refresh bounded log tails for the running guest.",
        actionTitle: "Probe QMP",
        action: .openConsole,
        severity: .ready
      )
    }

    if virtualMachine.status == .error {
      return VMReadinessSummary(
        title: "VM needs attention",
        detail: "Review metadata diagnostics and refresh launch readiness before starting again.",
        actionTitle: "Refresh Runner",
        action: .refreshRunner,
        severity: .blocked
      )
    }

    if let bootMediaStatusError {
      return VMReadinessSummary(
        title: "Boot media check failed",
        detail: bootMediaStatusError,
        actionTitle: "Refresh Boot Media",
        action: .refreshBootMedia,
        severity: .attention
      )
    }

    guard let bootMediaStatus else {
      return VMReadinessSummary(
        title: "Check boot media",
        detail: "Load installer, kernel, initrd, or restore-image metadata before launch prep.",
        actionTitle: "Check Boot Media",
        action: .refreshBootMedia,
        severity: .informational
      )
    }

    if let missingBootMedia = bootMediaStatus.entries.first(where: { !$0.exists }) {
      return VMReadinessSummary(
        title: "\(missingBootMedia.kind.title) missing",
        detail: missingBootMedia.path,
        actionTitle: "Refresh Boot Media",
        action: .refreshBootMedia,
        severity: .blocked
      )
    }

    if let snapshotChainError {
      return VMReadinessSummary(
        title: "Disk chain check failed",
        detail: snapshotChainError,
        actionTitle: "Prepare Disk",
        action: .prepareDisk,
        severity: .attention
      )
    }

    if let snapshotChain, !snapshotChain.activeDisk.exists {
      return VMReadinessSummary(
        title: snapshotChain.readinessTitle,
        detail: snapshotChain.activeDisk.path,
        actionTitle: "Prepare Disk",
        action: .prepareDisk,
        severity: .blocked
      )
    }

    if diskVerification != nil || diskInspection != nil || diskCreation != nil
      || diskPreparation != nil
    {
      return launchSummary(
        virtualMachine: virtualMachine,
        runnerStatus: runnerStatus,
        runnerStatusError: runnerStatusError,
        preRunLaunchReadiness: preRunLaunchReadiness
      )
    }

    if let runnerStatus, runnerStatus.launchReadiness?.ready == true {
      return launchSummary(
        virtualMachine: virtualMachine,
        runnerStatus: runnerStatus,
        runnerStatusError: runnerStatusError,
        preRunLaunchReadiness: preRunLaunchReadiness
      )
    }

    return VMReadinessSummary(
      title: "Prepare primary disk",
      detail: "Create or inspect the primary qcow2 metadata before generating a launch spec.",
      actionTitle: "Prepare Disk",
      action: .prepareDisk,
      severity: .attention
    )
  }

  private static func launchSummary(
    virtualMachine: VirtualMachine,
    runnerStatus: RunnerStatus?,
    runnerStatusError: String?,
    preRunLaunchReadiness: LaunchReadiness? = nil
  ) -> VMReadinessSummary {
    if let runnerStatusError {
      return VMReadinessSummary(
        title: "Launch readiness failed",
        detail: runnerStatusError,
        actionTitle: "Prepare Launch",
        action: .prepareRun,
        severity: .attention
      )
    }

    guard let runnerStatus else {
      if let preRunLaunchReadiness {
        if preRunLaunchReadiness.ready {
          return VMReadinessSummary(
            title: "Launch checks clear",
            detail: "Pre-run launch readiness is clear; prepare launch metadata before starting.",
            actionTitle: "Prepare Launch",
            action: .prepareRun,
            severity: .ready
          )
        }

        if let blocker = preRunLaunchReadiness.blockers.first {
          return VMReadinessSummary(
            title: preRunLaunchReadiness.title,
            detail: blocker.summary,
            actionTitle: "Prepare Launch",
            action: .prepareRun,
            severity: .blocked
          )
        }

        return VMReadinessSummary(
          title: preRunLaunchReadiness.title,
          detail: "Pre-run launch readiness is available; prepare launch metadata before starting.",
          actionTitle: "Prepare Launch",
          action: .prepareRun,
          severity: .informational
        )
      }

      return VMReadinessSummary(
        title: "Prepare launch",
        detail: "Generate the backend launch spec and readiness blockers before starting.",
        actionTitle: "Prepare Launch",
        action: .prepareRun,
        severity: .informational
      )
    }

    if runnerStatus.launchReadiness?.ready == true {
      return VMReadinessSummary(
        title: "Launch checks clear",
        detail: runnerStatus.dryRun
          ? "Dry-run launch metadata is ready; review before starting the backend."
          : "Launch metadata is ready for the selected backend.",
        actionTitle: virtualMachine.primaryActionTitle,
        action: .primaryAction,
        severity: .ready
      )
    }

    if let blocker = runnerStatus.launchReadiness?.blockers.first {
      return VMReadinessSummary(
        title: runnerStatus.launchReadinessTitle,
        detail: blocker.summary,
        actionTitle: "Prepare Launch",
        action: .prepareRun,
        severity: .blocked
      )
    }

    return VMReadinessSummary(
      title: runnerStatus.launchReadinessTitle,
      detail: runnerStatus.launchSpecPath ?? runnerStatus.commandLine,
      actionTitle: "Refresh Runner",
      action: .refreshRunner,
      severity: .informational
    )
  }
}

struct VMReadinessReport: Equatable {
  var vm: String
  var mode: VirtualMachine.EngineMode
  var state: VirtualMachine.Status
  var metadataOnly: Bool
  var liveE2ERequired: Bool
  var liveEvidence: VMLiveEvidence? = nil
  var evidenceRequirements: [VMEvidenceRequirement]
  var bootMedia: BootMediaStatus?
  var bootMediaError: String?
  var snapshotChain: VMSnapshotChain?
  var snapshotChainError: String?
  var runner: RunnerStatus?
  var runnerError: String?
  var preRunLaunchReadiness: LaunchReadiness? = nil
  var qmpSupervisor: QMPSupervisor? = nil
  var blockers: [String]
  var notes: [String]

  var readinessTitle: String {
    guard blockers.isEmpty else {
      return "Blocked (\(blockers.count))"
    }

    if liveE2ERequired {
      if !pendingRequiredEvidence.isEmpty {
        return evidenceReadinessTitle
      }

      guard liveEvidenceVerifiedForDisplay else {
        return "Live E2E evidence required"
      }
    }

    return metadataOnly ? "Metadata checks clear" : "Readiness clear"
  }

  var pendingRequiredEvidence: [VMEvidenceRequirement] {
    evidenceRequirements.filter { $0.required && !$0.proven }
  }

  var evidenceReadinessTitle: String {
    let pendingCount = pendingRequiredEvidence.count
    guard pendingCount > 0 else {
      return "Evidence complete"
    }

    if pendingCount == 1 {
      return "1 evidence check pending"
    }

    return "\(pendingCount) evidence checks pending"
  }

  var liveEvidenceReadinessTitle: String {
    let pendingCount = pendingRequiredEvidence.count
    guard pendingCount > 0 else {
      return "Live evidence complete"
    }

    if pendingCount == 1 {
      return "1 live evidence check pending"
    }

    return "\(pendingCount) live evidence checks pending"
  }

  var liveEvidenceVerifiedForDisplay: Bool {
    guard let liveEvidence else {
      return false
    }

    return liveEvidence.hasAnyVerifiedEvidence
  }
}

struct VMLiveEvidence: Equatable {
  var path: String
  var backend: String
  var vmName: String
  var bootMode: String
  var diskFormat: String
  var network: String
  var serialSentinelRequired: Bool
  var serialSentinelProven: Bool
  var graphicalBootProgressProven: Bool = false
  var viewerEvidenceProven: Bool = false
  var qmpEvidenceProven: Bool = false
  var guestToolsEffectsProven: Bool = false
  var summary: String

  var proofItems: [VMLiveEvidenceProofItem] {
    var items: [VMLiveEvidenceProofItem] = []

    if serialSentinelRequired {
      items.append(
        VMLiveEvidenceProofItem(
          kind: "serial-sentinel",
          title: "Serial sentinel",
          proven: serialSentinelProven,
          detail: serialSentinelProven
            ? "Serial boot sentinel captured"
            : "Serial boot sentinel pending"
        )
      )
    }

    items.append(
      VMLiveEvidenceProofItem(
        kind: "graphical-boot-progress",
        title: "Boot progress",
        proven: graphicalBootProgressProven,
        detail: graphicalBootProgressProven
          ? "Graphical boot progress captured"
          : "Graphical boot progress pending"
      )
    )

    items.append(
      VMLiveEvidenceProofItem(
        kind: "viewer",
        title: "Viewer",
        proven: viewerEvidenceProven,
        detail: viewerEvidenceProven
          ? "Graphical viewer evidence captured"
          : "Graphical viewer evidence pending"
      )
    )

    items.append(
      VMLiveEvidenceProofItem(
        kind: "qmp",
        title: "QMP",
        proven: qmpEvidenceProven,
        detail: qmpEvidenceProven
          ? "QMP console/control evidence captured"
          : "QMP console/control evidence pending"
      )
    )

    items.append(
      VMLiveEvidenceProofItem(
        kind: "guest-tools",
        title: "Guest tools",
        proven: guestToolsEffectsProven,
        detail: guestToolsEffectsProven
          ? "Guest-tools effects captured"
          : "Guest-tools effects pending"
      )
    )

    return items
  }

  var interactiveConsoleEvidenceProven: Bool {
    serialSentinelProven || viewerEvidenceProven
  }

  var liveBootProgressProven: Bool {
    serialSentinelProven || graphicalBootProgressProven
  }

  var hasAnyVerifiedEvidence: Bool {
    liveBootProgressProven || interactiveConsoleEvidenceProven || qmpEvidenceProven
      || guestToolsEffectsProven
  }

  var title: String {
    let consoleEvidenceProven = interactiveConsoleEvidenceProven

    if !consoleEvidenceProven {
      if qmpEvidenceProven && guestToolsEffectsProven {
        return "Preserved QMP console and guest-tools evidence verified; graphical/serial proof pending"
      }

      if qmpEvidenceProven {
        return "Preserved QMP console evidence verified; graphical/serial proof pending"
      }

      return "Preserved live evidence recorded; console proof pending"
    }

    if consoleEvidenceProven && guestToolsEffectsProven {
      return "Preserved live and guest-tools evidence verified"
    }

    return "Preserved live evidence verified"
  }

  var detail: String {
    let consoleEvidenceProven = interactiveConsoleEvidenceProven
    let console =
      consoleEvidenceProven
      ? "graphical/serial console evidence proven"
      : "graphical/serial console evidence pending"
    let viewer = viewerEvidenceProven ? "viewer evidence proven" : "viewer evidence pending"
    let bootProgress =
      liveBootProgressProven ? "boot progress proven" : "boot progress pending"
    let qmp = qmpEvidenceProven ? "QMP evidence proven" : "QMP evidence pending"
    let guestTools = guestToolsEffectsProven ? "guest-tools effects proven" : "guest-tools effects pending"
    return "\(backend), \(bootMode), \(diskFormat), \(network), \(console), \(bootProgress), \(viewer), \(qmp), \(guestTools)"
  }
}

struct VMLiveEvidenceProofItem: Equatable, Identifiable {
  var kind: String
  var title: String
  var proven: Bool
  var detail: String

  var id: String { kind }
  var status: String { proven ? "proven" : "pending" }
}

struct VMEvidenceRequirement: Equatable, Identifiable {
  var kind: String
  var required: Bool
  var proven: Bool
  var note: String

  var id: String { kind }

  var title: String {
    switch kind {
    case "live-boot": return "Live boot"
    case "console": return "Console"
    case "guest-tools-effects": return "Guest tools effects"
    default: return kind
    }
  }
}

enum LifecyclePlanAction: String, CaseIterable, Codable, Equatable {
  case suspend
  case resume

  var title: String {
    switch self {
    case .suspend: return "Suspend"
    case .resume: return "Resume"
    }
  }
}

struct LifecyclePlan: Equatable {
  var vm: String
  var action: LifecyclePlanAction
  var currentState: VirtualMachine.Status
  var targetState: VirtualMachine.Status
  var backend: String
  var metadataOnly: Bool
  var executable: Bool
  var qmpCommand: String?
  var socketPath: String?
  var socketAvailable: Bool
  var blockers: [String]
  var notes: [String]
}

struct OpenPortPlan: Equatable {
  var vm: String
  var scheme: String
  var host: String
  var guestPort: UInt16
  var hostPort: UInt16
  var url: String
  var command: [String]
}

struct NetworkPlan: Equatable {
  var vm: String
  var backend: String
  var mode: String
  var hostname: String
  var dryRun: Bool
  var executable: Bool
  var portForwards: [VMPortForward]
  var capabilities: NetworkCapabilities?
  var blockers: [NetworkPlanBlocker]
  var notes: [String]

  init(
    vm: String,
    backend: String,
    mode: String,
    hostname: String,
    dryRun: Bool,
    executable: Bool,
    portForwards: [VMPortForward],
    capabilities: NetworkCapabilities?,
    blockers: [NetworkPlanBlocker],
    notes: [String]
  ) {
    self.vm = vm
    self.backend = backend
    self.mode = mode
    self.hostname = hostname
    self.dryRun = dryRun
    self.executable = executable
    self.portForwards = portForwards
    self.capabilities = capabilities
    self.blockers = blockers
    self.notes = notes
  }

  init(
    vm: String,
    mode: String,
    backend: String,
    hostname: String,
    capabilities: [String],
    portForwards: [VMPortForward],
    blockers: [String],
    notes: [String]
  ) {
    let capabilitySet = Set(capabilities)
    self.init(
      vm: vm,
      backend: backend,
      mode: mode,
      hostname: hostname,
      dryRun: true,
      executable: blockers.isEmpty,
      portForwards: portForwards,
      capabilities: NetworkCapabilities(
        guestOutbound: capabilitySet.contains("guest-outbound"),
        hostToGuest: capabilitySet.contains("host-to-guest")
          || capabilitySet.contains("port-forward"),
        guestToHost: capabilitySet.contains("guest-to-host")
          || capabilitySet.contains("guest-tools-ip"),
        hostVisibleHostname: capabilitySet.contains("host-visible-hostname"),
        supportsPortForwarding: capabilitySet.contains("port-forward"),
        requiresPrivilegedHelper: capabilitySet.contains("privileged-helper")
      ),
      blockers: blockers.map { NetworkPlanBlocker(code: $0, message: $0) },
      notes: notes
    )
  }
}

struct NetworkCapabilities: Equatable {
  var guestOutbound: Bool
  var hostToGuest: Bool
  var guestToHost: Bool
  var hostVisibleHostname: Bool
  var supportsPortForwarding: Bool
  var requiresPrivilegedHelper: Bool
}

struct NetworkPlanBlocker: Equatable {
  var code: String
  var message: String
}

struct SSHPlan: Equatable {
  var vm: String
  var user: String
  var host: String
  var port: UInt16
  var source: Source
  var command: [String]

  enum Source: String, Codable, Equatable {
    case guestToolsIP = "guest-tools-ip"
    case portForward = "port-forward"
    case unknown

    init(from decoder: Decoder) throws {
      let container = try decoder.singleValueContainer()
      let rawValue = try container.decode(String.self)
      self = Source(rawValue: rawValue) ?? .unknown
    }

    var title: String {
      switch self {
      case .guestToolsIP:
        return "Guest tools IP"
      case .portForward:
        return "Port forward"
      case .unknown:
        return "Unknown"
      }
    }
  }

  var commandLine: String { command.joined(separator: " ") }
}

struct VMPortForwardList: Equatable {
  var vm: String
  var forwards: [VMPortForward]
}

struct VMPortForward: Identifiable, Equatable {
  var host: UInt16
  var guest: UInt16

  var id: String { "\(host)-\(guest)" }
}

struct VMSharedFolderList: Equatable {
  var vm: String
  var sharedFolders: [VMSharedFolder]
}

struct VMSharedFolder: Identifiable, Equatable {
  var name: String
  var hostPath: String
  var readOnly: Bool
  var hostPathToken: String

  var id: String {
    "\(name)-\(hostPathToken)"
  }
}

enum VMSnapshotKind: String, Codable, Equatable {
  case disk
  case suspend
  case applicationConsistent = "application-consistent"
  case unknown

  init(from decoder: Decoder) throws {
    let container = try decoder.singleValueContainer()
    let rawValue = try container.decode(String.self)
    self = VMSnapshotKind(rawValue: rawValue) ?? .unknown
  }

  func encode(to encoder: Encoder) throws {
    var container = encoder.singleValueContainer()
    try container.encode(rawValue)
  }

  var title: String {
    switch self {
    case .disk:
      return "Disk"
    case .suspend:
      return "Suspend"
    case .applicationConsistent:
      return "Application-consistent"
    case .unknown:
      return "Snapshot"
    }
  }
}

struct VMSnapshot: Identifiable, Equatable {
  var name: String
  var kind: VMSnapshotKind
  var createdAtUnix: UInt64
  var vmState: VirtualMachine.Status

  var id: String { name }
}

struct SnapshotRestoreResult: Equatable {
  var snapshot: String
  var restoredAtUnix: UInt64
  var restoredState: VirtualMachine.Status
  var activeDisk: SnapshotActiveDisk?
  var suspendImage: SnapshotSuspendImage?
}

struct SnapshotActiveDisk: Equatable {
  var source: String
  var snapshot: String?
  var path: String
  var format: String
  var exists: Bool
  var activatedAtUnix: UInt64
}

struct SnapshotSuspendImage: Equatable {
  var snapshot: String
  var imagePath: String
  var imageFormat: String
  var imageExists: Bool
  var preparedAtUnix: UInt64
}

struct VMSnapshotChain: Equatable {
  var activeDisk: VMActiveDisk
  var disks: [VMSnapshotDisk]

  var readinessTitle: String {
    if activeDisk.exists {
      return disks.isEmpty ? "Primary disk active" : "Chain ready"
    }

    return "Active disk missing"
  }
}

struct VMActiveDisk: Equatable {
  var source: String
  var snapshot: String?
  var path: String
  var format: String
  var exists: Bool
  var activatedAtUnix: UInt64

  var sourceTitle: String {
    switch source {
    case "primary":
      return "Primary"
    case "snapshot-overlay":
      return "Snapshot overlay"
    case "snapshot-backing":
      return "Snapshot backing"
    default:
      return source
    }
  }
}

struct VMSnapshotDisk: Identifiable, Equatable {
  var snapshot: String
  var overlayPath: String
  var overlayFormat: String
  var overlayExists: Bool
  var backingPath: String
  var backingFormat: String
  var backingExists: Bool
  var createCommand: [String]
  var preparedAtUnix: UInt64

  var id: String { snapshot }
  var createCommandLine: String { createCommand.joined(separator: " ") }
}

struct VMSnapshotDiskCreation: Equatable {
  var snapshot: String
  var disk: VMSnapshotDisk
  var command: [String]
  var executed: Bool
  var exitStatus: String?
  var stdout: String
  var stderr: String
  var createdAtUnix: UInt64

  var commandLine: String { command.joined(separator: " ") }
}

indirect enum DiskMetadataValue: Decodable, Equatable {
  case null
  case bool(Bool)
  case int(Int64)
  case double(Double)
  case string(String)
  case array([DiskMetadataValue])
  case object([String: DiskMetadataValue])

  init(from decoder: Decoder) throws {
    let container = try decoder.singleValueContainer()

    if container.decodeNil() {
      self = .null
    } else if let value = try? container.decode(Bool.self) {
      self = .bool(value)
    } else if let value = try? container.decode(Int64.self) {
      self = .int(value)
    } else if let value = try? container.decode(Double.self) {
      self = .double(value)
    } else if let value = try? container.decode(String.self) {
      self = .string(value)
    } else if let value = try? container.decode([DiskMetadataValue].self) {
      self = .array(value)
    } else {
      self = .object(try container.decode([String: DiskMetadataValue].self))
    }
  }

  var prettyPrinted: String {
    guard let value = jsonObject,
      JSONSerialization.isValidJSONObject(value),
      let data = try? JSONSerialization.data(
        withJSONObject: value,
        options: [.prettyPrinted, .sortedKeys]
      ),
      let text = String(data: data, encoding: .utf8)
    else {
      return description
    }

    return text
  }

  private var jsonObject: Any? {
    switch self {
    case .null:
      return NSNull()
    case .bool(let value):
      return value
    case .int(let value):
      return value
    case .double(let value):
      return value
    case .string(let value):
      return value
    case .array(let values):
      return values.map { $0.jsonObject ?? NSNull() }
    case .object(let values):
      return Dictionary(
        uniqueKeysWithValues: values.map { ($0.key, $0.value.jsonObject ?? NSNull()) })
    }
  }

  private var description: String {
    switch self {
    case .null:
      return "null"
    case .bool(let value):
      return String(value)
    case .int(let value):
      return String(value)
    case .double(let value):
      return String(value)
    case .string(let value):
      return value
    case .array(let values):
      return "[" + values.map(\.description).joined(separator: ", ") + "]"
    case .object(let values):
      return "{"
        + values.keys.sorted().map { "\($0): \(values[$0]?.description ?? "null")" }
        .joined(separator: ", ") + "}"
    }
  }
}

struct VMDiskVerification: Equatable {
  var activeDisk: VMActiveDisk
  var command: [String]
  var exitStatus: String
  var report: String
  var reportValue: DiskMetadataValue
  var stdout: String
  var stderr: String
  var verifyDurationMicroseconds: UInt64
  var verifiedAtUnix: UInt64

  var commandLine: String { command.joined(separator: " ") }
}

struct VMDiskCreation: Equatable {
  var preparation: DiskPreparation
  var command: [String]?
  var executed: Bool
  var exitStatus: String?
  var stdout: String
  var stderr: String
  var createdAtUnix: UInt64

  var commandLine: String {
    command?.joined(separator: " ") ?? "None"
  }
}

struct VMDiskInspection: Equatable {
  var preparation: DiskPreparation
  var command: [String]
  var exitStatus: String
  var info: String
  var infoValue: DiskMetadataValue
  var stdout: String
  var stderr: String
  var inspectDurationMicroseconds: UInt64
  var inspectedAtUnix: UInt64

  var commandLine: String { command.joined(separator: " ") }
}

struct VMDiskCompaction: Equatable {
  var preparation: DiskPreparation
  var activeDisk: VMActiveDisk
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

  var commandLine: String { command.joined(separator: " ") }

  var savedBytes: UInt64 {
    originalSizeBytes > compactedSizeBytes ? originalSizeBytes - compactedSizeBytes : 0
  }
}

struct DiskPreparation: Equatable {
  var path: String
  var format: String
  var size: String
  var sizeBytes: UInt64?
  var exists: Bool
  var created: Bool
  var createCommand: [String]?
  var preparedAtUnix: UInt64

  var createCommandLine: String? {
    createCommand?.joined(separator: " ")
  }
}

struct VMMetadataRepair: Equatable {
  var vm: String
  var bundle: String
  var repaired: Bool
  var actions: [VMMetadataRepairAction]
  var repairedAtUnix: UInt64
}

struct VMMetadataRepairAction: Identifiable, Equatable {
  var action: String
  var path: String
  var detail: String

  var id: String { "\(action)-\(path)-\(detail)" }
}

struct VMManifestMigration: Equatable {
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
}

struct DiagnosticBundle: Equatable {
  var vm: String
  var source: String
  var output: String
  var files: [String]
  var createdAtUnix: UInt64

  var fileCountTitle: String {
    let count = files.count
    return count == 1 ? "1 file" : "\(count) files"
  }

  var fileListTitle: String {
    files.isEmpty ? "None" : files.joined(separator: ", ")
  }
}

struct VMExportMetadata: Equatable {
  var vm: String
  var source: String
  var output: String
  var archiveFormat: String
  var copiedFileCount: UInt64
  var copiedFiles: [String]
  var manifestPreserved: Bool
  var metadataPreserved: Bool
  var exportedAtUnix: UInt64
}

struct VMImportMetadata: Equatable {
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
}

struct PerformanceMeasurement: Identifiable, Equatable {
  var name: String
  var value: UInt64
  var unit: String
  var source: String
  var metadataOnly: Bool

  var id: String {
    "\(name)-\(source)-\(unit)"
  }

  var valueTitle: String {
    "\(value) \(unit)"
  }
}

struct PerformanceBaseline: Equatable {
  var vm: String
  var source: String
  var output: String
  var artifact: String
  var createdAtUnix: UInt64
  var metadataOnly: Bool
  var state: VirtualMachine.Status
  var runner: RunnerStatus?
  var guestTools: GuestToolsStatus
  var metrics: GuestToolsMetrics?
  var measurements: [PerformanceMeasurement]
  var notes: [String]
}

struct PerformanceSample: Equatable {
  var vm: String
  var source: String
  var output: String
  var artifact: String
  var probe: String
  var probes: [String]
  var artifactBytes: UInt64
  var iterations: UInt16
  var sync: Bool
  var iterationResults: [PerformanceSampleIteration]
  var createdAtUnix: UInt64
  var state: VirtualMachine.Status
  var runner: RunnerStatus?
  var guestTools: GuestToolsStatus
  var metrics: GuestToolsMetrics?
  var measurements: [PerformanceMeasurement]
  var notes: [String]
}

struct PerformanceSampleIteration: Identifiable, Equatable {
  var iteration: UInt16
  var probe: String
  var bytes: UInt64
  var writeLatencyMicroseconds: UInt64
  var sync: Bool

  var id: UInt16 { iteration }

  var bytesTitle: String {
    "\(bytes) bytes"
  }

  var writeLatencyTitle: String {
    "\(writeLatencyMicroseconds) us"
  }
}

struct BootMediaStatus: Equatable {
  var vm: String
  var entries: [BootMediaStatusEntry]
}

struct BootMediaStatusEntry: Identifiable, Equatable {
  enum Kind: String, Equatable {
    case installerImage = "installer-image"
    case kernel
    case initrd
    case macosRestoreImage = "macos-restore-image"
    case unknown

    var title: String {
      switch self {
      case .installerImage:
        return "Installer image"
      case .kernel:
        return "Kernel"
      case .initrd:
        return "Initrd"
      case .macosRestoreImage:
        return "macOS restore image"
      case .unknown:
        return "Boot media"
      }
    }

    var isImportable: Bool {
      self != .unknown
    }
  }

  var kind: Kind
  var path: String
  var exists: Bool
  var sizeBytes: UInt64?
  var lastImport: BootMediaImportMetadata?
  var lastVerification: BootMediaVerificationMetadata?
  var lastDownloadPlan: BootMediaDownloadPlanMetadata?
  var lastDownload: BootMediaDownloadResultMetadata?

  var id: String {
    "\(kind.rawValue)-\(path)"
  }
}

struct BootMediaImportMetadata: Equatable {
  var vm: String
  var kind: BootMediaStatusEntry.Kind
  var source: String
  var destination: String
  var bytes: UInt64
  var replaced: Bool
  var importedAtUnix: UInt64
}

struct BootMediaVerificationMetadata: Equatable {
  var vm: String
  var kind: BootMediaStatusEntry.Kind
  var path: String
  var bytes: UInt64
  var expectedSHA256: String
  var actualSHA256: String
  var verified: Bool
  var verifiedAtUnix: UInt64
}

struct BootMediaDownloadPlanMetadata: Equatable {
  var vm: String
  var kind: BootMediaStatusEntry.Kind
  var url: String
  var destination: String
  var exists: Bool
  var bytes: UInt64?
  var expectedSHA256: String?
  var plannedAtUnix: UInt64
}

struct BootMediaDownloadResultMetadata: Equatable {
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
}

struct GuestToolsStatus: Equatable {
  var vm: String
  var tools: String
  var tokenCreatedAtUnix: UInt64
  var capabilities: [GuestToolsCapability]
  var approvedSharedFolders: [GuestToolsApprovedSharedFolder] = []
  var runtime: GuestToolsRuntime?

  var connected: Bool {
    runtime?.connected == true
  }

  var primaryIPAddress: String? {
    runtime?.guestIPAddresses.first?.address
  }

  var networkReadinessTitle: String {
    guard connected else {
      return "Waiting for tools"
    }

    if primaryIPAddress != nil {
      return "Guest IP ready"
    }

    return "Connected, no IP"
  }

  var displayReadinessTitle: String {
    let policyAllowsDisplayResize = capabilities.contains { $0.name == "display-resize" }
    let runtimeSupportsDisplayResize = runtime?.capabilities.contains("display-resize") == true

    if runtimeSupportsDisplayResize {
      return "Runtime advertises resize"
    }

    if policyAllowsDisplayResize {
      return "Policy allows resize"
    }

    return "Not advertised"
  }

  var clipboardReadinessTitle: String {
    let policyAllowsClipboard = capabilities.contains { $0.name == "clipboard" }
    let runtimeSupportsClipboard = runtime?.capabilities.contains("clipboard") == true

    if runtimeSupportsClipboard {
      return "Runtime advertises clipboard"
    }

    if policyAllowsClipboard {
      return "Policy allows clipboard"
    }

    return "Not advertised"
  }

  var sharedFoldersReadinessTitle: String {
    let policyAllowsSharedFolders = capabilities.contains { $0.name == "shared-folders" }
    let runtimeSupportsSharedFolders = runtime?.capabilities.contains("shared-folders") == true

    if runtimeSupportsSharedFolders {
      return "Runtime advertises shares"
    }

    if policyAllowsSharedFolders {
      return "Policy allows shares"
    }

    return "Not advertised"
  }

  var approvedSharedFoldersTitle: String {
    guard !approvedSharedFolders.isEmpty else {
      return "None approved"
    }

    let count = approvedSharedFolders.count
    return count == 1 ? "Approved (1)" : "Approved (\(count))"
  }

  func mountReadinessTitle(for folder: GuestToolsApprovedSharedFolder) -> String {
    if mountedSharedFolder(named: folder.name) != nil {
      return "Mounted"
    }

    guard connected else {
      return "Waiting for tools"
    }

    if runtime?.capabilities.contains("shared-folders") == true {
      return "Mount command available"
    }

    return "Not advertised"
  }

  func canMountApprovedSharedFolder(_ folder: GuestToolsApprovedSharedFolder) -> Bool {
    connected
      && runtime?.capabilities.contains("shared-folders") == true
      && mountedSharedFolder(named: folder.name) == nil
  }

  func canUnmountApprovedSharedFolder(_ folder: GuestToolsApprovedSharedFolder) -> Bool {
    connected
      && runtime?.capabilities.contains("shared-folders") == true
      && mountedSharedFolder(named: folder.name) != nil
  }

  func mountedSharedFolder(named name: String) -> GuestToolsSharedFolder? {
    runtime?.sharedFolders.first { $0.name == name && $0.mountedAtUnix != nil }
  }
}

struct GuestToolsToken: Equatable {
  var vm: String
  var createdAtUnix: UInt64
  var tokenLength: Int

  var hasToken: Bool {
    tokenLength > 0
  }
}

enum GuestToolsLinuxCommandTransport: String, Equatable {
  case socket
  case device

  var title: String {
    switch self {
    case .socket: return "Socket"
    case .device: return "Device"
    }
  }
}

struct GuestToolsLinuxCommand: Equatable {
  var vm: String
  var transport: GuestToolsLinuxCommandTransport
  var command: [String]
  var tokenFile: String
  var capabilities: [String]

  var commandLine: String {
    command.joined(separator: " ")
  }
}

struct GuestToolsProvisioning: Equatable {
  var token: GuestToolsToken?
  var deviceCommand: GuestToolsLinuxCommand?
  var socketCommand: GuestToolsLinuxCommand?
}

struct GuestToolsCapability: Identifiable, Equatable {
  var name: String
  var maxVersion: UInt16
  var enabledBy: String

  var id: String {
    "\(name)-\(maxVersion)-\(enabledBy)"
  }
}

struct GuestToolsRuntime: Equatable {
  var connected: Bool
  var guestOS: String?
  var agentVersion: String?
  var capabilities: [String]
  var lastHeartbeatAtUnix: UInt64?
  var guestIPAddresses: [GuestToolsIPAddress]
  var sharedFolders: [GuestToolsSharedFolder]
  var metrics: GuestToolsMetrics?
  var lastClipboard: GuestClipboardSnapshot? = nil
  var lastCommandResult: GuestToolsCommandResult? = nil
  var updatedAtUnix: UInt64
  var agentUpdate: GuestToolsAgentUpdate? = nil
}

struct GuestClipboardSnapshot: Equatable {
  var text: String
  var updatedAtUnix: UInt64
}

struct GuestToolsAgentUpdate: Equatable {
  var currentVersion: String
  var availableVersion: String
  var downloadURL: String?
  var signature: String?
  var observedAtUnix: UInt64
}

struct GuestToolsCommandResult: Equatable {
  var requestID: String
  var capability: String?
  var ok: Bool
  var errorCode: String?
  var message: String?
  var result: GuestToolsCommandPayload? = nil
  var metadata: GuestToolsCommandPayload? = nil
  var completedAtUnix: UInt64
}

struct GuestToolsCommandPayload: Equatable, Decodable {
  var value: GuestToolsJSONValue

  init(value: GuestToolsJSONValue) {
    self.value = value
  }

  init(from decoder: Decoder) throws {
    value = try GuestToolsJSONValue(from: decoder)
  }

  var displayText: String {
    value.displayText
  }
}

enum GuestToolsJSONValue: Equatable, Decodable {
  case null
  case bool(Bool)
  case number(String)
  case string(String)
  case array([GuestToolsJSONValue])
  case object([String: GuestToolsJSONValue])

  init(from decoder: Decoder) throws {
    let container = try decoder.singleValueContainer()
    if container.decodeNil() {
      self = .null
    } else if let value = try? container.decode(Bool.self) {
      self = .bool(value)
    } else if let value = try? container.decode(Int64.self) {
      self = .number(String(value))
    } else if let value = try? container.decode(Double.self) {
      self = .number(Self.formatNumber(value))
    } else if let value = try? container.decode(String.self) {
      self = .string(value)
    } else if let value = try? container.decode([GuestToolsJSONValue].self) {
      self = .array(value)
    } else {
      self = .object(try container.decode([String: GuestToolsJSONValue].self))
    }
  }

  var displayText: String {
    switch self {
    case .null:
      return "null"
    case .bool(let value):
      return value ? "true" : "false"
    case .number(let value):
      return value
    case .string(let value):
      return value
    case .array(let values):
      return "[" + values.map(\.displayText).joined(separator: ", ") + "]"
    case .object(let values):
      return values.keys.sorted().map { key in
        "\(key): \(values[key]?.displayText ?? "null")"
      }.joined(separator: ", ")
    }
  }

  private static func formatNumber(_ value: Double) -> String {
    if value.rounded() == value {
      return String(Int64(value))
    }
    return String(value)
  }
}

struct GuestToolsIPAddress: Identifiable, Equatable {
  var address: String
  var interface: String?

  var id: String {
    "\(interface ?? "default")-\(address)"
  }
}

struct GuestToolsMetrics: Equatable {
  var cpuPercent: UInt8
  var memoryUsedMiB: UInt64
  var updatedAtUnix: UInt64
}

struct GuestToolsApprovedSharedFolder: Identifiable, Equatable {
  var name: String
  var hostPath: String
  var hostPathToken: String
  var readOnly: Bool
  var approval: String

  var id: String {
    "\(name)-\(hostPathToken)"
  }
}

struct GuestToolsSharedFolder: Identifiable, Equatable {
  var name: String
  var hostPathToken: String
  var mountedAtUnix: UInt64? = nil

  var id: String {
    name
  }
}

struct GuestToolsCommandDispatch: Equatable {
  var vm: String
  var requestID: String?
  var pendingCommands: Int
}

struct QMPStatus: Equatable {
  var socketPath: String
  var available: Bool
  var status: String?
  var running: Bool?
  var supervisor: QMPSupervisor? = nil

  var readinessTitle: String {
    guard available else {
      return "QMP socket unavailable"
    }

    if let status, !status.isEmpty {
      if running == true && status.caseInsensitiveCompare("running") == .orderedSame {
        return status
      }
      return running == true ? "\(status), running" : status
    }

    return "QMP socket available"
  }
}

struct ConsoleCapability: Equatable {
  var graphicalViewerAvailable: Bool
  var qmpDiagnosticsAvailable: Bool
  var boundedLogTailsAvailable: Bool

  static func evaluate(
    for virtualMachine: VirtualMachine,
    qemuLaunchPlan: QemuLaunchPlan? = nil
  ) -> ConsoleCapability {
    ConsoleCapability(
      graphicalViewerAvailable: qemuLaunchPlan?.viewerEndpoint != nil,
      qmpDiagnosticsAvailable: virtualMachine.canOpenConsole,
      boundedLogTailsAvailable: true
    )
  }

  var title: String {
    graphicalViewerAvailable ? "Graphical console advertised" : "Diagnostics only"
  }

  var detail: String {
    if graphicalViewerAvailable {
      return "Verify viewer output separately from QMP diagnostics and bounded logs."
    }

    if qmpDiagnosticsAvailable {
      return "No graphical viewer is embedded; use QMP diagnostics and bounded log tails."
    }

    return "No graphical viewer is embedded; bounded log tails remain available."
  }

  var graphicalViewerTitle: String {
    graphicalViewerAvailable ? "Available" : "Not embedded"
  }

  var qmpDiagnosticsTitle: String {
    qmpDiagnosticsAvailable ? "Probe available" : "Requires running VM"
  }

  var boundedLogTailsTitle: String {
    boundedLogTailsAvailable ? "Available" : "Unavailable"
  }

  var actionTitle: String {
    graphicalViewerAvailable ? "Open VNC" : "Probe QMP"
  }

  var actionSystemImage: String {
    graphicalViewerAvailable ? "display" : "point.3.connected.trianglepath.dotted"
  }
}

struct QMPSupervisor: Equatable {
  var events: [QMPSupervisorEvent]
  var terminalEvent: QMPSupervisorEvent?
  var envelopesRead: Int
  var limitReached: Bool
  var updatedAtUnix: UInt64

  var summaryTitle: String {
    if let terminalEvent {
      return "\(events.count) events, terminal \(terminalEvent.name)"
    }
    return "\(events.count) events"
  }
}

struct QMPSupervisorEvent: Equatable {
  var name: String
}

struct SnapshotPreflightStatus: Equatable {
  var vm: String
  var consistency: SnapshotConsistency
  var backendFreezeThawSupported: Bool
  var guestToolsConnected: Bool
  var capabilities: [String]
  var ready: Bool
  var blockers: [SnapshotPreflightBlocker]
  var checkedAtUnix: UInt64?

  var readinessTitle: String {
    guard backendFreezeThawSupported else {
      return "Scaffold only"
    }

    if ready {
      return "Preflight ready"
    }

    let count = blockers.count
    return count == 1 ? "Blocked (1)" : "Blocked (\(count))"
  }
}

enum SnapshotConsistency: String, Codable, Equatable {
  case crashConsistent = "crash-consistent"
  case applicationConsistent = "application-consistent"

  var title: String {
    switch self {
    case .crashConsistent:
      return "Crash-consistent"
    case .applicationConsistent:
      return "Application-consistent"
    }
  }
}

struct SnapshotPreflightBlocker: Identifiable, Equatable {
  var code: String
  var message: String
  var path: String?

  var id: String {
    "\(code)-\(path ?? message)"
  }
}

enum VMLogKind: String, Codable, Equatable {
  case qemu
  case serial

  var title: String {
    switch self {
    case .qemu: return "QEMU"
    case .serial: return "Serial"
    }
  }
}

struct VMLogView: Equatable {
  var vm: String
  var kind: VMLogKind
  var path: String
  var exists: Bool
  var bytes: UInt64
  var returnedBytes: UInt64
  var truncated: Bool
  var content: String
}

struct QemuLaunchPlan: Equatable {
  var program: String
  var args: [String]

  var command: [String] {
    [program] + args
  }

  var commandLine: String {
    command.joined(separator: " ")
  }

  var viewerEndpoint: URL? {
    guard let displayIndex = args.firstIndex(of: "-display") else {
      return nil
    }

    let valueIndex = args.index(after: displayIndex)
    guard args.indices.contains(valueIndex) else {
      return nil
    }

    let display = args[valueIndex]
    guard display.hasPrefix("vnc=:") else {
      return nil
    }

    let displayNumberText = display
      .dropFirst("vnc=:".count)
      .split(separator: ",", maxSplits: 1, omittingEmptySubsequences: false)
      .first ?? ""
    guard
      let displayNumber = Int(displayNumberText),
      displayNumber >= 0
    else {
      return nil
    }

    return URL(string: "vnc://127.0.0.1:\(5900 + displayNumber)")
  }
}

enum GuestToolsAgentCommand: Equatable {
  case setClipboard(text: String)
  case resizeDisplay(width: UInt32, height: UInt32, scale: UInt16)
  case unmountShare(name: String)
  case fileDropStart(transferID: String, fileName: String, sizeBytes: UInt64)
  case fileDropChunk(transferID: String, chunkIndex: UInt32, dataBase64: String)
  case fileDropComplete(transferID: String)
  case listApplications
  case launchApplication(id: String)
  case listWindows
  case focusWindow(id: String)
  case closeWindow(id: String)
  case timeSync(unixEpochMillis: UInt64)

  var requiredRuntimeCapability: String {
    switch self {
    case .setClipboard:
      return "clipboard"
    case .resizeDisplay:
      return "display-resize"
    case .unmountShare:
      return "shared-folders"
    case .fileDropStart, .fileDropChunk, .fileDropComplete:
      return "drag-drop"
    case .listApplications, .launchApplication:
      return "applications"
    case .listWindows, .focusWindow, .closeWindow:
      return "windows"
    case .timeSync:
      return "time-sync"
    }
  }
}

struct RunnerStatus: Equatable {
  var engine: String
  var pid: UInt32?
  var command: [String]
  var logPath: String
  var startedAtUnix: UInt64
  var dryRun: Bool
  var launchSpecPath: String?
  var launchReadiness: LaunchReadiness?
  var qmpSupervisor: QMPSupervisor? = nil
  var guestTools: GuestToolsRunnerStatus? = nil

  var commandLine: String {
    command.joined(separator: " ")
  }

  var launchReadinessTitle: String {
    guard let launchReadiness else {
      return "Not reported"
    }

    if launchReadiness.ready {
      return "Ready"
    }

    let count = launchReadiness.blockers.count
    return count == 1 ? "Blocked (1)" : "Blocked (\(count))"
  }
}

struct GuestToolsRunnerStatus: Equatable {
  var transport: String
  var channelName: String
  var socketPath: String
  var tokenPath: String
  var tokenCreatedAtUnix: UInt64
}

struct LaunchReadiness: Equatable {
  var ready: Bool
  var blockers: [LaunchReadinessBlocker]

  var title: String {
    if ready {
      return "Ready"
    }

    let count = blockers.count
    return count == 1 ? "Blocked (1)" : "Blocked (\(count))"
  }
}

struct LaunchReadinessBlocker: Identifiable, Equatable {
  var code: String
  var message: String
  var path: String?
  var capability: String?

  var id: String {
    "\(code)-\(path ?? capability ?? message)"
  }

  var summary: String {
    var value = "\(code): \(message)"
    if let path {
      value += " (\(path))"
    } else if let capability {
      value += " (\(capability))"
    }
    return value
  }
}
