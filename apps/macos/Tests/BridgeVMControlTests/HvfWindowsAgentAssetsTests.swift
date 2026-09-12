import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfWindowsAgentAssetsTests: XCTestCase {
    private let fixedDate = Date(timeIntervalSince1970: 1_700_000_000)
    private let originalBytes = Data("required-agent-asset-v1\n".utf8)
    private let changedRecipe = "Windows 설치 레시피 또는 wimlib가 변경되었습니다."

    func testRequiredAgentPathsAreExactUniqueAndRequiredByPreflight() {
        let expected = [
            "scripts/win-assets/bvagent.ps1",
            "scripts/win-assets/bvagent-firstboot.ps1",
            "scripts/win-assets/bvagent-input.ps1",
            "scripts/win-assets/bvagent-unicode-input.cs",
            "scripts/win-assets/bvagent-key-input.cs",
            "scripts/win-assets/bvagent-pointer-input.cs",
            "scripts/win-assets/bvagent-task.ps1",
            "scripts/win-assets/bvagent-window-inventory.ps1",
            "scripts/win-assets/bv-window-inventory.cs",
        ]
        XCTAssertEqual(HvfWindowsAgentAssets.requiredPaths, expected)
        XCTAssertEqual(Set(HvfWindowsAgentAssets.requiredPaths).count, expected.count)
        XCTAssertEqual(HvfWindowsInstallPlan.installResourcePaths.filter {
            expected.contains($0)
        }, expected)
    }

    func testEveryAgentMutationInvalidatesCacheWithUnchangedSizeAndMtime() throws {
        try fixture { root, iso in
            let captured = plan(root: root, iso: iso)
            for relative in HvfWindowsAgentAssets.requiredPaths {
                let asset = root.appendingPathComponent(relative)
                var changed = originalBytes
                changed[0] ^= 0xff
                try write(changed, to: asset)
                let attributes = try FileManager.default.attributesOfItem(atPath: asset.path)
                XCTAssertEqual((attributes[.size] as? NSNumber)?.intValue,
                               originalBytes.count, relative)
                XCTAssertEqual(attributes[.modificationDate] as? Date, fixedDate, relative)
                XCTAssertNotEqual(plan(root: root, iso: iso).sourceCacheKey,
                                  captured.sourceCacheKey, relative)
                XCTAssertEqual(captured.validationError(), changedRecipe, relative)
                try write(originalBytes, to: asset)
                XCTAssertEqual(plan(root: root, iso: iso).sourceCacheKey,
                               captured.sourceCacheKey, relative)
            }
        }
    }

    func testEveryAgentMissingBeforeCaptureFailsResourcePreflight() throws {
        try fixture { root, iso in
            for relative in HvfWindowsAgentAssets.requiredPaths {
                let asset = root.appendingPathComponent(relative)
                try FileManager.default.removeItem(at: asset)
                let capturedWithoutAsset = plan(root: root, iso: iso)
                XCTAssertEqual(capturedWithoutAsset.validationError(),
                               "앱 설치 리소스가 없습니다: \(relative)", relative)
                try write(originalBytes, to: asset)
            }
        }
    }

    func testEveryAgentRemovedAfterCaptureInvalidatesRecipeUntilRestored() throws {
        try fixture { root, iso in
            let captured = plan(root: root, iso: iso)
            for relative in HvfWindowsAgentAssets.requiredPaths {
                let asset = root.appendingPathComponent(relative)
                try FileManager.default.removeItem(at: asset)
                XCTAssertEqual(captured.validationError(), changedRecipe, relative)
                XCTAssertNotEqual(plan(root: root, iso: iso).sourceCacheKey,
                                  captured.sourceCacheKey, relative)
                try write(originalBytes, to: asset)
                XCTAssertEqual(plan(root: root, iso: iso).sourceCacheKey,
                               captured.sourceCacheKey, relative)
            }
        }
    }

    private func plan(root: URL, iso: URL) -> HvfWindowsInstallPlan {
        HvfWindowsInstallPlan(
            repoRoot: root, libraryRoot: root.appendingPathComponent("library"),
            bundlePath: root.appendingPathComponent("bundle.vmbridge").path,
            slug: "agent-closure", request: HvfWindowsInstallRequest(
                isoPath: iso.path, diskGiB: 64,
                injectViogpu3d: false, driverPackageDir: nil))
    }

    private func write(_ data: Data, to url: URL) throws {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(),
                                                withIntermediateDirectories: true)
        try data.write(to: url)
        try FileManager.default.setAttributes([.modificationDate: fixedDate],
                                             ofItemAtPath: url.path)
    }

    private func fixture(_ body: (URL, URL) throws -> Void) throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-agent-assets-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        let iso = root.appendingPathComponent("synthetic.iso")
        try write(Data("synthetic-iso\n".utf8), to: iso)
        for relative in HvfWindowsInstallPlan.installResourcePaths {
            try write(originalBytes, to: root.appendingPathComponent(relative))
        }
        try body(root, iso)
    }
}
