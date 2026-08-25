import XCTest
@testable import BridgeVMControl
final class HvfWindowsProductInjectionPreflightTests: XCTestCase {
    func testProductInjectionFailsBeforeSourceInspectionOrVMBundleCreation() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let source = root.appendingPathComponent("source", isDirectory: true)
        let storage = root.appendingPathComponent("library", isDirectory: true)
        try FileManager.default.createDirectory(at: source, withIntermediateDirectories: true); try FileManager.default.createDirectory(at: storage, withIntermediateDirectories: true)
        let disk = source.appendingPathComponent("installed.raw"); let vars = source.appendingPathComponent("vars.fd"); let iso = source.appendingPathComponent("win.iso")
        try Data([1]).write(to: disk); try Data([2]).write(to: iso); XCTAssertTrue(FileManager.default.createFile(atPath: vars.path, contents: Data()))
        let handle = try FileHandle(forWritingTo: vars); try handle.truncate(atOffset: VMLibrary.windowsHVFVarsBytes); try handle.close()
        defer { try? FileManager.default.removeItem(at: root) }
        let blocker = try XCTUnwrap(VMLibrary.windowsHVFInjectionError(requested: true))
        XCTAssertTrue(blocker.contains(HvfWindowsDriverPreflight.provenanceBlocker)); XCTAssertNil(VMLibrary.windowsHVFInjectionError(requested: false))
        XCTAssertNil(VMLibrary.createWindowsHVF(name: "Blocked Import", targetDiskPath: disk.path, varsPath: vars.path, storageDir: storage, injectViogpu3d: true, persist: false))
        XCTAssertNil(VMLibrary.createWindowsHVFInstall(name: "Blocked Install", isoPath: iso.path, diskGiB: 64, injectViogpu3d: true, driverPackageDir: source.path, storageDir: storage, persist: false))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: storage.path), [])
    }
}
