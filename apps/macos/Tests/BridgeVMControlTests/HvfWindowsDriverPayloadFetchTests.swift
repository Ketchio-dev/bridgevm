import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfWindowsDriverPayloadFetchTests: XCTestCase {
    private let payloadFiles = [
        "network/netkvm.cat", "network/netkvm.inf", "network/netkvm.sys", "network/netkvmp.exe",
        "serial/vioser.cat", "serial/vioser.inf", "serial/vioser.sys",
        "storage/viostor.cat", "storage/viostor.inf", "storage/viostor.sys",
    ]

    func testBuildsTheExactPayloadAndManifestThenReusesTheVerifiedArchive() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let source = try fixture.source()
        let prepared = try HvfWindowsDriverPayloadFetch.prepare(source: source, root: fixture.cache)

        let payload = URL(fileURLWithPath: prepared.payloadDirectory)
        let entries = try FileManager.default.subpathsOfDirectory(atPath: payload.path)
        // Typed pieces: one long `+` chain is too slow for the CI compiler to type-check.
        var expectedEntries: [String] = ["network"]
        expectedEntries += Array(payloadFiles[0..<4])
        expectedEntries.append("serial")
        expectedEntries += Array(payloadFiles[4..<7])
        expectedEntries.append("storage")
        expectedEntries += Array(payloadFiles[7...])
        XCTAssertEqual(entries.sorted(), expectedEntries)
        for path in payloadFiles {
            let name = (path as NSString).lastPathComponent
            XCTAssertEqual(try Data(contentsOf: payload.appendingPathComponent(path)), Data("arm64 \(name)".utf8))
        }
        var expected = """
        schema\tbridgevm-windows-guest-payload-v1
        architecture\tarm64
        driver\tstorage\tstorage/viostor.inf\tstorage/viostor.cat\tstorage/viostor.sys
        driver\tserial\tserial/vioser.inf\tserial/vioser.cat\tserial/vioser.sys
        driver\tnetwork\tnetwork/netkvm.inf\tnetwork/netkvm.cat\tnetwork/netkvm.sys,network/netkvmp.exe

        """
        for path in payloadFiles {
            let digest = try XCTUnwrap(HvfWindowsInstallCacheIdentity.sha256File(payload.appendingPathComponent(path).path))
            expected += "file\t\(path)\t\(digest)\n"
        }
        XCTAssertEqual(try String(contentsOfFile: prepared.manifestPath, encoding: .utf8), expected)
        XCTAssertNil(HvfWindowsGuestPayloadIdentity.inspect(
            payloadDirectory: prepared.payloadDirectory, manifestPath: prepared.manifestPath).error)

        // A second click rebuilds from the kept archive without downloading again.
        try FileManager.default.removeItem(at: fixture.archive)
        try Data().write(to: payload.appendingPathComponent(".DS_Store"))
        XCTAssertEqual(try HvfWindowsDriverPayloadFetch.prepare(source: source, root: fixture.cache), prepared)
        XCTAssertEqual(try FileManager.default.subpathsOfDirectory(atPath: payload.path).sorted(), entries.sorted())
    }

    func testCLICreateWithFetchedDriversSealsThemForInstall() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        let iso = fixture.root.appendingPathComponent("synthetic.iso")
        try Data("synthetic iso".utf8).write(to: iso)
        let library = fixture.root.appendingPathComponent("library", isDirectory: true)
        var options = NativeCLICreateWindowsOptions(name: "Fetched", isoPath: iso.path, diskGiB: 64,
            memoryMiB: 6144, cpuCount: 2, resolution: .init(width: 1280, height: 800),
            networkEnabled: false, fetchDrivers: true)
        options.guestPayload = try HvfWindowsDriverPayloadFetch.prepare(source: fixture.source(), root: fixture.cache)
        let result = try NativeCLICreateWindows.create(options, libraryRoot: library)
        let config = try NativeLibraryReader.readConfig(rootURL: library, id: result.id)
        let request = try XCTUnwrap(HvfWindowsInstallRequest.load(bundlePath: config.bundlePath))
        XCTAssertEqual(request.guestPayloadDirectory, config.bundlePath + "/metadata/windows-guest-payload")
        XCTAssertNotNil(request.guestPayloadIdentity)
        XCTAssertTrue(result.guestDriversSealed)
        XCTAssertTrue(result.text.contains("Next: bridgevm app install \(result.id)"))
    }

    func testRefusesAnArchiveThatDoesNotMatchThePinnedSHA256() throws {
        let fixture = try Fixture()
        defer { fixture.remove() }
        var source = try fixture.source()
        source.sha256 = String(repeating: "0", count: 64)
        XCTAssertThrowsError(try HvfWindowsDriverPayloadFetch.prepare(source: source, root: fixture.cache)) { error in
            XCTAssertTrue((error as? HvfWindowsDriverPayloadFetch.Failure)?.message.contains("SHA-256") == true)
        }
        let folder = fixture.cache.appendingPathComponent(source.name)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: folder.path), [])
    }

    private struct Fixture {
        let root: URL
        let archive: URL
        var cache: URL { root.appendingPathComponent("cache", isDirectory: true) }

        init() throws {
            let fm = FileManager.default
            root = fm.temporaryDirectory.appendingPathComponent("driver-fetch-\(UUID().uuidString)", isDirectory: true)
            archive = root.appendingPathComponent("virtio-test.zip")
            let tree = root.appendingPathComponent("tree", isDirectory: true)
            let names = ["viostor.inf", "viostor.cat", "viostor.sys", "vioser.inf", "vioser.cat", "vioser.sys",
                         "netkvm.inf", "netkvm.cat", "netkvm.sys", "netkvmp.exe",
                         "netkvmco.exe", "viostor.pdb", "viogpudo.sys"]
            for folder in ["Win11/ARM64", "Win11/amd64", "Win10/ARM64"] {
                let directory = tree.appendingPathComponent(folder, isDirectory: true)
                try fm.createDirectory(at: directory, withIntermediateDirectories: true)
                let tag = folder == "Win11/ARM64" ? "arm64" : "decoy"
                for name in names { try Data("\(tag) \(name)".utf8).write(to: directory.appendingPathComponent(name)) }
            }
            let zip = Process()
            zip.executableURL = URL(fileURLWithPath: "/usr/bin/zip")
            zip.currentDirectoryURL = tree
            zip.arguments = ["-q", "-r", "-X", archive.path, "Win11", "Win10"]
            try zip.run()
            zip.waitUntilExit()
            XCTAssertEqual(zip.terminationStatus, 0)
        }

        func source() throws -> HvfWindowsDriverPayloadFetch.Source {
            let size = try XCTUnwrap(archive.resourceValues(forKeys: [.fileSizeKey]).fileSize)
            let digest = try XCTUnwrap(HvfWindowsInstallCacheIdentity.sha256File(archive.path))
            return .init(url: archive.absoluteString, bytes: size, sha256: digest, folder: "Win11/ARM64/")
        }

        func remove() { try? FileManager.default.removeItem(at: root) }
    }
}
