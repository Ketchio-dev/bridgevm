import XCTest
@testable import BridgeVMControl

/// T19 r3 (2026-10-22) imported read-only sealed inputs. The native helper clones on APFS and
/// a clone keeps the source's mode, so the VM's own disk and vars stayed read-only, launch
/// readiness refused them, and Start stayed disabled.
final class HvfMediaImportOwnershipTests: XCTestCase {
    func testReadOnlySelectionImportsAsWritableVMMedia() throws {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appendingPathComponent("hvf-media-owner-" + UUID().uuidString)
        try fm.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? fm.removeItem(at: root) }
        let disk = root.appendingPathComponent("source.raw"), vars = root.appendingPathComponent("source.fd")
        try Data([1, 2, 3]).write(to: disk)
        try Data().write(to: vars)
        let handle = try FileHandle(forWritingTo: vars)
        try handle.truncate(atOffset: FirstRunImport.requiredVarsBytes); try handle.close()
        for source in [disk, vars] { try fm.setAttributes([.posixPermissions: 0o444], ofItemAtPath: source.path) }
        let bundle = root.appendingPathComponent("bundle")
        try fm.createDirectory(at: bundle, withIntermediateDirectories: false)
        let toDisk = bundle.appendingPathComponent("disk.raw"), toVars = bundle.appendingPathComponent("vars.fd")

        try HvfMediaImport.copy(disk: disk.path, vars: vars.path, toDisk: toDisk, toVars: toVars,
                                helper: HvfMediaImportTestSupport.helper)

        XCTAssertEqual(try mode(toDisk), 0o600)
        XCTAssertEqual(try mode(toVars), 0o600)
        XCTAssertEqual(try mode(disk), 0o444, "the selection stays read-only")
        XCTAssertEqual(try mode(vars), 0o444, "the selection stays read-only")
        XCTAssertEqual(try Data(contentsOf: toDisk), Data([1, 2, 3]))
        let config = HvfEngineConfig(targetDiskPath: toDisk.path, uefiVarsPath: toVars.path,
            evidenceDir: root.appendingPathComponent("evidence").path, watchdogMs: nil, ramMiB: 4096, smpCpus: 4,
            clipboardSync: false, shareHostDir: nil, shareGuestDir: nil, virtioNet: false, virtioGpu3d: false,
            nvmeBufferedIO: false, ctlFilePath: root.appendingPathComponent("control.ctl").path)
        let codes = config.readiness(repoRoot: root).launchBlockers.map(\.code)
        XCTAssertFalse(codes.contains("target-disk-access"), "\(codes)")
        XCTAssertFalse(codes.contains("uefi-vars-access"), "\(codes)")
    }

    private func mode(_ url: URL) throws -> Int {
        try ((FileManager.default.attributesOfItem(atPath: url.path)[.posixPermissions] as? NSNumber)?.intValue ?? -1) & 0o777
    }
}
