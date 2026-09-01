import XCTest
@testable import BridgeVMControl

final class HvfWindowsInstallPathTests: XCTestCase {
    func testSourceBuildPreservesLibraryAndISOPathsContainingSpaces() throws {
        let temp = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-spaced-install-\(UUID())", isDirectory: true)
        try FileManager.default.createDirectory(at: temp, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: temp) }
        let iso = temp.appendingPathComponent("Windows 11.iso")
        try Data(count: 2048).write(to: iso)
        let library = temp.appendingPathComponent("Application Support/BridgeVM/vms")
        let plan = HvfWindowsInstallPlan(
            repoRoot: URL(fileURLWithPath: "/repo"), libraryRoot: library,
            bundlePath: temp.appendingPathComponent("bundle").path, slug: "plain",
            request: HvfWindowsInstallRequest(
                isoPath: iso.path, diskGiB: 64,
                injectViogpu3d: false, driverPackageDir: nil))
        let source = plan.sourceBuildCommand()
        XCTAssertEqual(source.environment["ISO"], iso.path)
        XCTAssertEqual(source.environment["OUT"], plan.sourceImagePath)
        XCTAssertTrue(plan.sourceImagePath.contains("Application Support"))
        XCTAssertEqual(source.arguments,
                       ["/bin/bash", "scripts/build-hvf-windows-scripted-source.sh"])
    }
}
