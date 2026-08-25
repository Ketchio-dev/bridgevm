import XCTest
@testable import BridgeVMControl
final class HvfWindowsImportInjectionPreflightTests: XCTestCase {
    func testImportInjectionFailsBeforeCreatingOrMutatingVMBundle() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let storage = root.appendingPathComponent("library", isDirectory: true)
        let disk = root.appendingPathComponent("installed.raw"); let vars = root.appendingPathComponent("vars.fd")
        try FileManager.default.createDirectory(at: storage, withIntermediateDirectories: true)
        try Data([1]).write(to: disk)
        XCTAssertTrue(FileManager.default.createFile(atPath: vars.path, contents: Data()))
        let handle = try FileHandle(forWritingTo: vars)
        try handle.truncate(atOffset: VMLibrary.windowsHVFVarsBytes); try handle.close()
        defer { try? FileManager.default.removeItem(at: root) }
        let blocker = try XCTUnwrap(VMLibrary.windowsHVFImportInjectionError(requested: true))
        XCTAssertTrue(blocker.contains(HvfWindowsDriverPreflight.provenanceBlocker))
        XCTAssertNil(VMLibrary.windowsHVFImportInjectionError(requested: false))
        XCTAssertNil(VMLibrary.createWindowsHVF(
            name: "Blocked Import", targetDiskPath: disk.path, varsPath: vars.path,
            storageDir: storage, injectViogpu3d: true, persist: false))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: storage.path), [])
    }
}
