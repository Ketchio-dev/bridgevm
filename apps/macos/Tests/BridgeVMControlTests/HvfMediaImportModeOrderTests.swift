import XCTest
@testable import BridgeVMControl

/// The imported disk and vars become owner-writable while they are still inside the import's private workspace, so
/// no mode change is ever made through the VM library path, where a swapped link would redirect it.
final class HvfMediaImportModeOrderTests: XCTestCase {
    func testMediaAreOwnerWritableBeforeTheyLeaveTheWorkspace() throws {
        let fm = ModeRecordingFileManager()
        let root = fm.temporaryDirectory.appendingPathComponent("hvf-media-order-" + UUID().uuidString)
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
        fm.recording = true

        try HvfMediaImport.copy(disk: disk.path, vars: vars.path, toDisk: toDisk, toVars: toVars,
                                helper: HvfMediaImportTestSupport.helper, fileManager: fm)

        XCTAssertEqual(fm.movedModes, [toDisk.path: 0o600, toVars.path: 0o600], "mode of each member as it was moved")
        XCTAssertTrue(fm.libraryModeChanges(under: bundle.path).isEmpty, "\(fm.libraryModeChanges(under: bundle.path))")
    }
}

private final class ModeRecordingFileManager: FileManager {
    var recording = false
    private(set) var movedModes: [String: Int] = [:]
    private(set) var modeChanges: [String] = []

    override func moveItem(atPath srcPath: String, toPath dstPath: String) throws {
        if recording, let mode = attributesOfItemIfPresent(srcPath)?[.posixPermissions] as? NSNumber {
            movedModes[dstPath] = mode.intValue & 0o777
        }
        try super.moveItem(atPath: srcPath, toPath: dstPath)
    }

    override func setAttributes(_ attributes: [FileAttributeKey: Any], ofItemAtPath path: String) throws {
        if recording, attributes[.posixPermissions] != nil { modeChanges.append(path) }
        try super.setAttributes(attributes, ofItemAtPath: path)
    }

    func libraryModeChanges(under directory: String) -> [String] {
        modeChanges.filter { $0.hasPrefix(directory + "/") }
    }

    private func attributesOfItemIfPresent(_ path: String) -> [FileAttributeKey: Any]? {
        try? attributesOfItem(atPath: path)
    }
}
