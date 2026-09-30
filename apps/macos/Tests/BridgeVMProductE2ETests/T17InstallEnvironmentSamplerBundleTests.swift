import Foundation
import XCTest
@testable import BridgeVMProductE2E

/// The packaged installer writes run.log into the VM bundle's private staging
/// directory, so the install-timeout sampler has to read it there.
final class T17InstallEnvironmentSamplerBundleTests: XCTestCase {
    func testSamplerReadsRunLogFromTheBundleStagingEvidence() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("T17 sampler " + UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let bundle = root.appendingPathComponent("bundle.vmbridge", isDirectory: true)
        let sampler = T17InstallEnvironmentSampler(bundlePath: bundle.path)
        XCTAssertEqual(sampler.capture().logStatus, .unavailable)
        let evidence = bundle.appendingPathComponent("metadata/hvf-install-staging/evidence", isDirectory: true)
        try FileManager.default.createDirectory(
            at: evidence, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        let log = Data("BVINSTALL DISM APPLY\n".utf8)
        try log.write(to: evidence.appendingPathComponent("run.log"))

        XCTAssertEqual(sampler.capture().logStatus, .present)
        XCTAssertEqual(sampler.capture().logBytes, UInt64(log.count))
        XCTAssertEqual(sampler.markers()?.dism, 1)
    }
}
