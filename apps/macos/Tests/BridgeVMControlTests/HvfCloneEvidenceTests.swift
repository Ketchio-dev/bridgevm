import XCTest
@testable import BridgeVMControl

final class HvfCloneEvidenceTests: XCTestCase {
    /// A duplicate of a Windows that already ran the guest agent keeps `guest-agent-seen` in its
    /// fresh `logs/hvf`: otherwise its first start attaches the Guest Tools disk again and a slow
    /// agent is reported as missing tools. The rest of the source's evidence is archived.
    func testTheCopyKeepsTheAgentSeenMarkerAndArchivesTheRest() throws {
        let fm = FileManager.default
        let bundle = fm.temporaryDirectory.appendingPathComponent("clone-evidence-\(UUID().uuidString)")
        defer { try? fm.removeItem(at: bundle) }
        let evidence = bundle.appendingPathComponent("logs/hvf")
        try fm.createDirectory(at: evidence, withIntermediateDirectories: true)
        try Data().write(to: evidence.appendingPathComponent("guest-agent-seen"))
        try Data("source run".utf8).write(to: evidence.appendingPathComponent("run.log"))

        XCTAssertTrue(VMLibrary.archiveCopiedEvidence(in: bundle, now: Date(timeIntervalSince1970: 42)))

        XCTAssertTrue(fm.fileExists(atPath: evidence.appendingPathComponent("guest-agent-seen").path))
        XCTAssertFalse(fm.fileExists(atPath: evidence.appendingPathComponent("run.log").path))
        let archive = bundle.appendingPathComponent("logs/hvf-source-copy-42")
        XCTAssertEqual(try Data(contentsOf: archive.appendingPathComponent("run.log")), Data("source run".utf8))
    }

    func testACopyWhoseSourceNeverSawTheAgentStaysWithoutTheMarker() throws {
        let fm = FileManager.default
        let bundle = fm.temporaryDirectory.appendingPathComponent("clone-evidence-\(UUID().uuidString)")
        defer { try? fm.removeItem(at: bundle) }
        try fm.createDirectory(at: bundle.appendingPathComponent("logs/hvf"), withIntermediateDirectories: true)

        XCTAssertTrue(VMLibrary.archiveCopiedEvidence(in: bundle))

        XCTAssertFalse(fm.fileExists(atPath: bundle.appendingPathComponent("logs/hvf/guest-agent-seen").path))
    }
}
