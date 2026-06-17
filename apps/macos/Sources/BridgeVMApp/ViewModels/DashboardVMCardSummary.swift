import Foundation

struct DashboardVMCardSummary: Equatable {
  var subtitle: String
  var metadataItems: [String]

  init(
    virtualMachine: VirtualMachine,
    readinessReport: VMReadinessReport? = nil,
    guestToolsStatus: GuestToolsStatus?,
    snapshots: [VMSnapshot],
    portForwardList: VMPortForwardList?
  ) {
    subtitle = "\(virtualMachine.guest) - \(virtualMachine.mode.title)"
    var items: [String] = []
    if let readiness = Self.readinessSummary(for: readinessReport) {
      items.append(readiness)
    }
    if let guestToolsEffects = Self.guestToolsEffectsSummary(for: readinessReport) {
      items.append(guestToolsEffects)
    }
    if let serialEvidence = Self.serialEvidenceSummary(for: readinessReport) {
      items.append(serialEvidence)
    }
    if let bootProgressEvidence = Self.bootProgressEvidenceSummary(for: readinessReport) {
      items.append(bootProgressEvidence)
    }
    if let viewerEvidence = Self.viewerEvidenceSummary(for: readinessReport) {
      items.append(viewerEvidence)
    }
    if let qmpEvidence = Self.qmpEvidenceSummary(for: readinessReport) {
      items.append(qmpEvidence)
    }
    items.append(Self.snapshotSummary(for: snapshots))
    items.append(Self.toolsSummary(for: virtualMachine, status: guestToolsStatus))
    items.append(Self.forwardingSummary(for: portForwardList))
    metadataItems = items
  }

  private static func readinessSummary(for report: VMReadinessReport?) -> String? {
    guard let report else {
      return nil
    }

    if !report.blockers.isEmpty {
      return "Readiness blocked: \(report.blockers.count)"
    }

    if report.metadataOnly && report.liveE2ERequired {
      if report.evidenceReadinessTitle == "Evidence complete" {
        guard report.liveEvidenceVerifiedForDisplay else {
          return "Metadata checks clear; live E2E evidence still required"
        }
        return "Metadata checks clear; live evidence verified"
      }
      let pendingTitles = report.pendingRequiredEvidence.map(\.title)
      let evidenceTitle = report.liveEvidenceReadinessTitle.lowercased()
      if pendingTitles.isEmpty {
        return "Metadata checks clear; \(evidenceTitle)"
      }
      return "Metadata checks clear; \(evidenceTitle): \(pendingTitles.joined(separator: ", "))"
    }

    return report.readinessTitle
  }

  private static func guestToolsEffectsSummary(for report: VMReadinessReport?) -> String? {
    guard
      let requirement = report?.evidenceRequirements.first(where: {
        $0.kind == "guest-tools-effects" && $0.required
      })
    else {
      return nil
    }

    return "\(requirement.title) \(requirement.proven ? "proven" : "unproven")"
  }

  private static func serialEvidenceSummary(for report: VMReadinessReport?) -> String? {
    guard
      let liveEvidence = report?.liveEvidence,
      liveEvidence.serialSentinelRequired
    else {
      return nil
    }

    return liveEvidence.serialSentinelProven
      ? "Serial evidence proven"
      : "Serial evidence pending"
  }

  private static func viewerEvidenceSummary(for report: VMReadinessReport?) -> String? {
    guard let liveEvidence = report?.liveEvidence else {
      return nil
    }

    return liveEvidence.viewerEvidenceProven
      ? "Viewer evidence proven"
      : "Viewer evidence pending"
  }

  private static func bootProgressEvidenceSummary(for report: VMReadinessReport?) -> String? {
    guard let liveEvidence = report?.liveEvidence else {
      return nil
    }

    return liveEvidence.liveBootProgressProven
      ? "Boot progress evidence proven"
      : "Boot progress evidence pending"
  }

  private static func qmpEvidenceSummary(for report: VMReadinessReport?) -> String? {
    guard let liveEvidence = report?.liveEvidence else {
      return nil
    }

    return liveEvidence.qmpEvidenceProven
      ? "QMP evidence proven"
      : "QMP evidence pending"
  }

  private static func snapshotSummary(for snapshots: [VMSnapshot]) -> String {
    guard
      let newest = snapshots.max(by: {
        if $0.createdAtUnix == $1.createdAtUnix {
          return $0.name < $1.name
        }

        return $0.createdAtUnix < $1.createdAtUnix
      })
    else {
      return "No snapshots"
    }

    return "Last snapshot: \(newest.name)"
  }

  private static func toolsSummary(
    for virtualMachine: VirtualMachine,
    status: GuestToolsStatus?
  ) -> String {
    guard let status else {
      return "Tools unknown"
    }

    if status.connected {
      return "Tools connected"
    }

    if status.tools == "required" || virtualMachine.mode == .fast {
      return "Tools missing"
    }

    return "Tools optional"
  }

  private static func forwardingSummary(for portForwardList: VMPortForwardList?) -> String {
    guard let portForwardList else {
      return "Forwards unknown"
    }

    switch portForwardList.forwards.count {
    case 0:
      return "No forwarded services"
    case 1:
      guard let forward = portForwardList.forwards.first else {
        return "No forwarded services"
      }

      return "Forwarded \(forward.host)->\(forward.guest)"
    default:
      return "\(portForwardList.forwards.count) forwarded services"
    }
  }
}
