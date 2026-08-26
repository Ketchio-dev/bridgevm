import XCTest
@testable import BridgeVMControl

final class HvfWindowsDriverPreflightTests: XCTestCase {
    func testMissingAndTestSigningReportsFailClosedWithExactBlockers() throws {
        let package = try makePackage()
        XCTAssertEqual(HvfWindowsDriverPreflight.inspect(packageDirectory: package.path).blocker,
                       "signing-report-missing")
        try report("""
        finalization_complete=true
        signing_mode=test
        test_signing_required=true
        sys_kernel_policy_verified=false
        cat_kernel_policy_verified=false
        """, in: package)
        let before = try FileManager.default.contentsOfDirectory(atPath: package.path).sorted()
        let result = HvfWindowsDriverPreflight.inspect(packageDirectory: package.path)
        XCTAssertEqual(result.blocker, "test-signing-blocked-by-secure-boot")
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: package.path).sorted(), before)
    }

    func testUnsignedKernelPolicyReportCannotAuthorizeInjection() throws {
        let package = try makePackage()
        try Data().write(to: package.appendingPathComponent("viogpu3d.inf"))
        try Data().write(to: package.appendingPathComponent("viogpu3d.sys"))
        try report("""
        finalization_complete=true
        signing_mode=kernel-policy
        test_signing_required=false
        sys_kernel_policy_verified=true
        cat_kernel_policy_verified=true
        """, in: package)
        let inspection = HvfWindowsDriverPreflight.inspect(packageDirectory: package.path)
        XCTAssertEqual(inspection.blocker, "kernel-policy-provenance-unverifiable"); XCTAssertNotNil(HvfWindowsInstallPlan.driverPackageError(package.path))
        try report("""
        finalization_complete=true
        signing_mode=kernel-policy
        test_signing_required=false
        sys_kernel_policy_verified=true
        cat_kernel_policy_verified=false
        """, in: package)
        XCTAssertEqual(HvfWindowsDriverPreflight.inspect(packageDirectory: package.path).blocker,
                       "kernel-policy-unverifiable")
    }

    private func makePackage() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }

    private func report(_ text: String, in package: URL) throws {
        try text.write(to: package.appendingPathComponent("bridgevm-finalization-report.txt"),
                       atomically: true, encoding: .ascii)
    }
}
