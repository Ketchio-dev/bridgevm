import XCTest

@testable import BridgeVMApp

final class VMLiveEvidenceTests: XCTestCase {
  func testProofItemsKeepQmpSeparateFromInteractiveConsoleProof() {
    let evidence = VMLiveEvidence(
      path: "/tmp/bridgevm-live-evidence",
      backend: "qemu",
      vmName: "Dev VM",
      bootMode: "compatibility",
      diskFormat: "qcow2",
      network: "nat",
      serialSentinelRequired: true,
      serialSentinelProven: false,
      viewerEvidenceProven: false,
      qmpEvidenceProven: true,
      guestToolsEffectsProven: false,
      summary: "QMP evidence captured"
    )

    XCTAssertFalse(evidence.interactiveConsoleEvidenceProven)
    XCTAssertTrue(evidence.hasAnyVerifiedEvidence)
    XCTAssertEqual(
      evidence.proofItems.map(\.kind),
      ["serial-sentinel", "viewer", "qmp", "guest-tools"]
    )
    XCTAssertEqual(evidence.proofItems.map(\.status), ["pending", "pending", "proven", "pending"])
  }

  func testProofItemsOmitSerialSentinelWhenItIsNotRequired() {
    let evidence = VMLiveEvidence(
      path: "/tmp/bridgevm-live-evidence",
      backend: "apple-virtualization-framework",
      vmName: "Dev VM",
      bootMode: "linux-kernel",
      diskFormat: "raw",
      network: "nat",
      serialSentinelRequired: false,
      serialSentinelProven: false,
      viewerEvidenceProven: true,
      qmpEvidenceProven: false,
      guestToolsEffectsProven: true,
      summary: "viewer and tools evidence captured"
    )

    XCTAssertTrue(evidence.interactiveConsoleEvidenceProven)
    XCTAssertEqual(evidence.proofItems.map(\.kind), ["viewer", "qmp", "guest-tools"])
    XCTAssertEqual(evidence.proofItems.map(\.status), ["proven", "pending", "proven"])
  }
}
