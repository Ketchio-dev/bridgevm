import XCTest

@testable import BridgeVMApp

final class DashboardVMCardSummaryTests: XCTestCase {
  func testSummaryIncludesNewestSnapshotMissingRequiredToolsAndSingleForward() {
    let virtualMachine = makeVirtualMachine(mode: .fast)
    let summary = DashboardVMCardSummary(
      virtualMachine: virtualMachine,
      guestToolsStatus: GuestToolsStatus(
        vm: virtualMachine.name,
        tools: "required",
        tokenCreatedAtUnix: 1_710_000_000,
        capabilities: [],
        runtime: nil
      ),
      snapshots: [
        VMSnapshot(
          name: "before-upgrade",
          kind: .disk,
          createdAtUnix: 1_710_000_100,
          vmState: .stopped
        ),
        VMSnapshot(
          name: "after-upgrade",
          kind: .disk,
          createdAtUnix: 1_710_000_300,
          vmState: .stopped
        ),
      ],
      portForwardList: VMPortForwardList(
        vm: virtualMachine.name,
        forwards: [VMPortForward(host: 2222, guest: 22)]
      )
    )

    XCTAssertEqual(summary.subtitle, "Ubuntu Arm64 - Fast Mode")
    XCTAssertEqual(
      summary.metadataItems,
      [
        "Last snapshot: after-upgrade",
        "Tools missing",
        "Forwarded 2222->22",
      ]
    )
  }

  func testSummaryDistinguishesUnknownMetadataFromEmptyMetadata() {
    let virtualMachine = makeVirtualMachine(mode: .compatibility)

    XCTAssertEqual(
      DashboardVMCardSummary(
        virtualMachine: virtualMachine,
        guestToolsStatus: nil,
        snapshots: [],
        portForwardList: nil
      ).metadataItems,
      [
        "No snapshots",
        "Tools unknown",
        "Forwards unknown",
      ]
    )

    XCTAssertEqual(
      DashboardVMCardSummary(
        virtualMachine: virtualMachine,
        guestToolsStatus: GuestToolsStatus(
          vm: virtualMachine.name,
          tools: "optional",
          tokenCreatedAtUnix: 1_710_000_000,
          capabilities: [],
          runtime: nil
        ),
        snapshots: [],
        portForwardList: VMPortForwardList(vm: virtualMachine.name, forwards: [])
      ).metadataItems,
      [
        "No snapshots",
        "Tools optional",
        "No forwarded services",
      ]
    )
  }

  func testSummaryReportsConnectedToolsAndForwardCount() {
    let virtualMachine = makeVirtualMachine(mode: .fast)
    let summary = DashboardVMCardSummary(
      virtualMachine: virtualMachine,
      guestToolsStatus: GuestToolsStatus(
        vm: virtualMachine.name,
        tools: "required",
        tokenCreatedAtUnix: 1_710_000_000,
        capabilities: [],
        runtime: GuestToolsRuntime(
          connected: true,
          guestOS: "ubuntu",
          agentVersion: "0.2.0",
          capabilities: ["clipboard"],
          lastHeartbeatAtUnix: 1_710_000_120,
          guestIPAddresses: [],
          sharedFolders: [],
          metrics: nil,
          updatedAtUnix: 1_710_000_121
        )
      ),
      snapshots: [],
      portForwardList: VMPortForwardList(
        vm: virtualMachine.name,
        forwards: [
          VMPortForward(host: 2222, guest: 22),
          VMPortForward(host: 8080, guest: 80),
        ]
      )
    )

    XCTAssertEqual(
      summary.metadataItems,
      [
        "No snapshots",
        "Tools connected",
        "2 forwarded services",
      ]
    )
  }

  func testSummaryKeepsLiveDiagnosticMetadataOutOfCardItems() {
    let virtualMachine = makeVirtualMachine(mode: .fast)
    let summary = DashboardVMCardSummary(
      virtualMachine: virtualMachine,
      guestToolsStatus: GuestToolsStatus(
        vm: virtualMachine.name,
        tools: "required",
        tokenCreatedAtUnix: 1_710_000_000,
        capabilities: [
          GuestToolsCapability(name: "clipboard", maxVersion: 1, enabledBy: "policy")
        ],
        runtime: GuestToolsRuntime(
          connected: true,
          guestOS: "ubuntu",
          agentVersion: "0.2.0",
          capabilities: ["clipboard", "shared-folders"],
          lastHeartbeatAtUnix: 1_710_000_120,
          guestIPAddresses: [
            GuestToolsIPAddress(address: "192.168.64.10", interface: "enp0s1")
          ],
          sharedFolders: [
            GuestToolsSharedFolder(
              name: "workspace",
              hostPathToken: "host-token-1",
              mountedAtUnix: 1_710_000_130
            )
          ],
          metrics: GuestToolsMetrics(
            cpuPercent: 42,
            memoryUsedMiB: 2048,
            updatedAtUnix: 1_710_000_140
          ),
          updatedAtUnix: 1_710_000_150
        )
      ),
      snapshots: [
        VMSnapshot(
          name: "preflight",
          kind: .disk,
          createdAtUnix: 1_710_000_100,
          vmState: .stopped
        )
      ],
      portForwardList: VMPortForwardList(
        vm: virtualMachine.name,
        forwards: [VMPortForward(host: 2222, guest: 22)]
      )
    )

    XCTAssertEqual(
      summary.metadataItems,
      [
        "Last snapshot: preflight",
        "Tools connected",
        "Forwarded 2222->22",
      ]
    )

    let joinedMetadata = summary.metadataItems.joined(separator: " ")
    XCTAssertFalse(joinedMetadata.contains("192.168.64.10"))
    XCTAssertFalse(joinedMetadata.contains("0.2.0"))
    XCTAssertFalse(joinedMetadata.contains("clipboard"))
    XCTAssertFalse(joinedMetadata.contains("host-token-1"))
    XCTAssertFalse(joinedMetadata.contains("2048"))
  }

  func testSummarySurfacesMetadataReadyWithLiveEvidencePending() {
    let virtualMachine = makeVirtualMachine(mode: .fast)
    let summary = DashboardVMCardSummary(
      virtualMachine: virtualMachine,
      readinessReport: VMReadinessReport(
        vm: virtualMachine.name,
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
        ],
        bootMedia: nil,
        bootMediaError: nil,
        snapshotChain: nil,
        snapshotChainError: nil,
        runner: nil,
        runnerError: nil,
        blockers: [],
        notes: ["daemon aggregate reused cached metadata"]
      ),
      guestToolsStatus: nil,
      snapshots: [],
      portForwardList: nil
    )

    XCTAssertEqual(
      summary.metadataItems.first,
      "Metadata checks clear; 2 live evidence checks pending: Live boot, Console")
    XCTAssertFalse(summary.metadataItems.joined(separator: " ").contains("daemon aggregate"))
  }

  func testSummarySurfacesLiveE2ERequirementWhenEvidenceRequirementsAreEmpty() {
    let virtualMachine = makeVirtualMachine(mode: .fast)
    let summary = DashboardVMCardSummary(
      virtualMachine: virtualMachine,
      readinessReport: VMReadinessReport(
        vm: virtualMachine.name,
        mode: .fast,
        state: .stopped,
        metadataOnly: true,
        liveE2ERequired: true,
        liveEvidence: nil,
        evidenceRequirements: [],
        bootMedia: nil,
        bootMediaError: nil,
        snapshotChain: nil,
        snapshotChainError: nil,
        runner: nil,
        runnerError: nil,
        blockers: [],
        notes: []
      ),
      guestToolsStatus: nil,
      snapshots: [],
      portForwardList: nil
    )

    let joinedMetadata = summary.metadataItems.joined(separator: " ")
    XCTAssertEqual(
      summary.metadataItems.first,
      "Metadata checks clear; live E2E evidence still required"
    )
    XCTAssertFalse(joinedMetadata.contains("live evidence complete"))
  }

  func testSummaryDoesNotMarkEmptyLiveEvidenceBundleComplete() {
    let virtualMachine = makeVirtualMachine(mode: .fast)
    let summary = DashboardVMCardSummary(
      virtualMachine: virtualMachine,
      readinessReport: VMReadinessReport(
        vm: virtualMachine.name,
        mode: .fast,
        state: .stopped,
        metadataOnly: true,
        liveE2ERequired: true,
        liveEvidence: VMLiveEvidence(
          path: "/store/vms/dev.vmbridge/metadata/live-evidence/latest",
          backend: "apple-virtualization-framework",
          vmName: "live-vz-linux",
          bootMode: "linux-kernel",
          diskFormat: "raw",
          network: "nat",
          serialSentinelRequired: true,
          serialSentinelProven: false,
          viewerEvidenceProven: false,
          qmpEvidenceProven: false,
          guestToolsEffectsProven: false,
          summary: "Bundle exists but proof flags are absent"
        ),
        evidenceRequirements: [],
        bootMedia: nil,
        bootMediaError: nil,
        snapshotChain: nil,
        snapshotChainError: nil,
        runner: nil,
        runnerError: nil,
        blockers: [],
        notes: []
      ),
      guestToolsStatus: nil,
      snapshots: [],
      portForwardList: nil
    )

    let joinedMetadata = summary.metadataItems.joined(separator: " ")
    XCTAssertEqual(
      summary.metadataItems.first,
      "Metadata checks clear; live E2E evidence still required"
    )
    XCTAssertFalse(joinedMetadata.contains("live evidence complete"))
    XCTAssertFalse(joinedMetadata.contains("live evidence verified"))
  }

  func testSummarySurfacesRequiredUnprovenGuestToolsEffectsEvidence() {
    let virtualMachine = makeVirtualMachine(mode: .fast)
    let summary = DashboardVMCardSummary(
      virtualMachine: virtualMachine,
      readinessReport: VMReadinessReport(
        vm: virtualMachine.name,
        mode: .fast,
        state: .stopped,
        metadataOnly: true,
        liveE2ERequired: true,
        evidenceRequirements: [
          VMEvidenceRequirement(
            kind: "guest-tools-effects",
            required: true,
            proven: false,
            note: "Guest tools effects evidence has not been captured."
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
      ),
      guestToolsStatus: nil,
      snapshots: [],
      portForwardList: nil
    )

    XCTAssertTrue(summary.metadataItems.contains("Guest tools effects unproven"))
  }

  func testSummarySurfacesRequiredProvenGuestToolsEffectsEvidence() {
    let virtualMachine = makeVirtualMachine(mode: .fast)
    let summary = DashboardVMCardSummary(
      virtualMachine: virtualMachine,
      readinessReport: VMReadinessReport(
        vm: virtualMachine.name,
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
      ),
      guestToolsStatus: nil,
      snapshots: [],
      portForwardList: nil
    )

    XCTAssertTrue(summary.metadataItems.contains("Guest tools effects proven"))
  }

  func testSummarySurfacesPendingViewerEvidenceWhenLiveEvidenceExists() {
    let virtualMachine = makeVirtualMachine(mode: .fast)
    let summary = DashboardVMCardSummary(
      virtualMachine: virtualMachine,
      readinessReport: VMReadinessReport(
        vm: virtualMachine.name,
        mode: .fast,
        state: .stopped,
        metadataOnly: true,
        liveE2ERequired: true,
        liveEvidence: makeLiveEvidence(viewerEvidenceProven: false),
        evidenceRequirements: [],
        bootMedia: nil,
        bootMediaError: nil,
        snapshotChain: nil,
        snapshotChainError: nil,
        runner: nil,
        runnerError: nil,
        blockers: [],
        notes: []
      ),
      guestToolsStatus: nil,
      snapshots: [],
      portForwardList: nil
    )

    XCTAssertTrue(summary.metadataItems.contains("Viewer evidence pending"))
    XCTAssertTrue(summary.metadataItems.contains("Serial evidence proven"))
    XCTAssertTrue(summary.metadataItems.contains("Boot progress evidence proven"))
    XCTAssertTrue(summary.metadataItems.contains("QMP evidence pending"))
  }

  func testSummarySurfacesProvenViewerEvidenceWhenLiveEvidenceExists() {
    let virtualMachine = makeVirtualMachine(mode: .fast)
    let summary = DashboardVMCardSummary(
      virtualMachine: virtualMachine,
      readinessReport: VMReadinessReport(
        vm: virtualMachine.name,
        mode: .fast,
        state: .stopped,
        metadataOnly: true,
        liveE2ERequired: true,
        liveEvidence: makeLiveEvidence(viewerEvidenceProven: true),
        evidenceRequirements: [],
        bootMedia: nil,
        bootMediaError: nil,
        snapshotChain: nil,
        snapshotChainError: nil,
        runner: nil,
        runnerError: nil,
        blockers: [],
        notes: []
      ),
      guestToolsStatus: nil,
      snapshots: [],
      portForwardList: nil
    )

    XCTAssertTrue(summary.metadataItems.contains("Viewer evidence proven"))
    XCTAssertTrue(summary.metadataItems.contains("Serial evidence proven"))
    XCTAssertTrue(summary.metadataItems.contains("Boot progress evidence proven"))
    XCTAssertTrue(summary.metadataItems.contains("QMP evidence pending"))
  }

  func testSummarySurfacesProvenQmpEvidenceWhenLiveEvidenceExists() {
    let virtualMachine = makeVirtualMachine(mode: .compatibility)
    let summary = DashboardVMCardSummary(
      virtualMachine: virtualMachine,
      readinessReport: VMReadinessReport(
        vm: virtualMachine.name,
        mode: .compatibility,
        state: .stopped,
        metadataOnly: true,
        liveE2ERequired: true,
        liveEvidence: makeLiveEvidence(
          viewerEvidenceProven: false,
          qmpEvidenceProven: true,
          backend: "qemu",
          bootMode: "compatibility",
          diskFormat: "qcow2",
          serialSentinelProven: false
        ),
        evidenceRequirements: [],
        bootMedia: nil,
        bootMediaError: nil,
        snapshotChain: nil,
        snapshotChainError: nil,
        runner: nil,
        runnerError: nil,
        blockers: [],
        notes: []
      ),
      guestToolsStatus: nil,
      snapshots: [],
      portForwardList: nil
    )

    XCTAssertTrue(summary.metadataItems.contains("Viewer evidence pending"))
    XCTAssertTrue(summary.metadataItems.contains("Serial evidence pending"))
    XCTAssertTrue(summary.metadataItems.contains("Boot progress evidence pending"))
    XCTAssertTrue(summary.metadataItems.contains("QMP evidence proven"))
  }

  func testSummarySurfacesPendingSerialEvidenceWhenLiveEvidenceExists() {
    let virtualMachine = makeVirtualMachine(mode: .fast)
    let summary = DashboardVMCardSummary(
      virtualMachine: virtualMachine,
      readinessReport: VMReadinessReport(
        vm: virtualMachine.name,
        mode: .fast,
        state: .stopped,
        metadataOnly: true,
        liveE2ERequired: true,
        liveEvidence: makeLiveEvidence(
          viewerEvidenceProven: false,
          serialSentinelProven: false
        ),
        evidenceRequirements: [],
        bootMedia: nil,
        bootMediaError: nil,
        snapshotChain: nil,
        snapshotChainError: nil,
        runner: nil,
        runnerError: nil,
        blockers: [],
        notes: []
      ),
      guestToolsStatus: nil,
      snapshots: [],
      portForwardList: nil
    )

    XCTAssertTrue(summary.metadataItems.contains("Serial evidence pending"))
    XCTAssertTrue(summary.metadataItems.contains("Boot progress evidence pending"))
    XCTAssertTrue(summary.metadataItems.contains("Viewer evidence pending"))
    XCTAssertTrue(summary.metadataItems.contains("QMP evidence pending"))
  }

  func testSummarySurfacesMetadataReadyWhenLiveEvidenceIsNotRequired() {
    let virtualMachine = makeVirtualMachine(mode: .fast)
    let summary = DashboardVMCardSummary(
      virtualMachine: virtualMachine,
      readinessReport: VMReadinessReport(
        vm: virtualMachine.name,
        mode: .fast,
        state: .stopped,
        metadataOnly: true,
        liveE2ERequired: false,
        evidenceRequirements: [],
        bootMedia: nil,
        bootMediaError: nil,
        snapshotChain: nil,
        snapshotChainError: nil,
        runner: nil,
        runnerError: nil,
        blockers: [],
        notes: ["cache warming reused daemon aggregate details"]
      ),
      guestToolsStatus: nil,
      snapshots: [],
      portForwardList: nil
    )

    XCTAssertEqual(summary.metadataItems.first, "Metadata checks clear")
    XCTAssertFalse(summary.metadataItems.joined(separator: " ").contains("cache warming"))
  }

  func testSummarySurfacesBlockedReadinessCountWithoutBlockerDetails() {
    let virtualMachine = makeVirtualMachine(mode: .compatibility)
    let summary = DashboardVMCardSummary(
      virtualMachine: virtualMachine,
      readinessReport: VMReadinessReport(
        vm: virtualMachine.name,
        mode: .compatibility,
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
        blockers: [
          "boot-media-missing:/Users/user/private/installers/linux.iso",
          "runner-missing:/tmp/bridgevm.sock",
        ],
        notes: []
      ),
      guestToolsStatus: nil,
      snapshots: [],
      portForwardList: nil
    )

    XCTAssertEqual(summary.metadataItems.first, "Readiness blocked: 2")
    let joinedMetadata = summary.metadataItems.joined(separator: " ")
    XCTAssertFalse(joinedMetadata.contains("/Users/user/private"))
    XCTAssertFalse(joinedMetadata.contains("/tmp/bridgevm.sock"))
  }

  private func makeVirtualMachine(mode: VirtualMachine.EngineMode) -> VirtualMachine {
    VirtualMachine(
      id: UUID(),
      name: "Dev VM",
      guest: "Ubuntu Arm64",
      status: .running,
      mode: mode,
      resources: .init(cpuCount: 4, memoryGB: 8, diskGB: 64),
      uptime: "12m",
      ipAddress: "192.168.64.10",
      lastStarted: nil,
      notes: "test"
    )
  }

  private func makeLiveEvidence(
    viewerEvidenceProven: Bool,
    qmpEvidenceProven: Bool = false,
    backend: String = "apple-virtualization-framework",
    bootMode: String = "linux-kernel",
    diskFormat: String = "raw",
    serialSentinelProven: Bool = true
  ) -> VMLiveEvidence {
    VMLiveEvidence(
      path: "/tmp/bridgevm-live-evidence",
      backend: backend,
      vmName: "Dev VM",
      bootMode: bootMode,
      diskFormat: diskFormat,
      network: "nat",
      serialSentinelRequired: true,
      serialSentinelProven: serialSentinelProven,
      viewerEvidenceProven: viewerEvidenceProven,
      qmpEvidenceProven: qmpEvidenceProven,
      guestToolsEffectsProven: false,
      summary: "live evidence captured"
    )
  }
}
