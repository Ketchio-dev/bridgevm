import XCTest
@testable import BridgeVMControl

final class HvfCloneSourcePathTests: XCTestCase {
    func testImplicitBundledDiskCopiesOrdinaryFile() throws {
        let f = try HvfCloneSourceFixture()
        try f.checkCopy(f.config(), allowed: true)
        XCTAssertEqual(try Data(contentsOf: f.disk), f.bytes)
    }
    func testSelectedDiskAliasToBundledRegularFileCopiesOrdinaryFile() throws {
        let f = try HvfCloneSourceFixture()
        var config = f.config()
        let link = try f.alias("selected-alias.raw", to: f.disk)
        config.diskPath = link.path
        try f.checkCopy(config, allowed: true)
        XCTAssertEqual(try f.fm.destinationOfSymbolicLink(atPath: link.path), f.disk.path)
        XCTAssertEqual(try Data(contentsOf: f.disk), f.bytes)
    }
    func testAncestorAliasCopiesOrdinaryBundle() throws {
        let f = try HvfCloneSourceFixture()
        var config = f.config()
        let link = try f.alias("ancestor", to: f.root)
        config.bundlePath = link.appendingPathComponent("source.bundle").path
        config.diskPath = f.disk.path
        try f.checkCopy(config, allowed: true)
        XCTAssertEqual(try Data(contentsOf: f.disk), f.bytes)
    }
    func testBundledDiskSymlinkToExternalMediaRefusesBeforeReservation() throws {
        let f = try HvfCloneSourceFixture()
        let external = f.root.appendingPathComponent("external.raw")
        try f.fm.moveItem(at: f.disk, to: external)
        try f.fm.createSymbolicLink(at: f.disk, withDestinationURL: external)
        try f.checkCopy(f.config(), allowed: false)
        XCTAssertEqual(try f.fm.destinationOfSymbolicLink(atPath: f.disk.path), external.path)
        XCTAssertEqual(try Data(contentsOf: external), f.bytes)
    }
    func testBundledDisksDirectorySymlinkRefusesBeforeReservation() throws {
        let f = try HvfCloneSourceFixture()
        let disks = f.disk.deletingLastPathComponent(), external = f.root.appendingPathComponent("external-disks")
        try f.fm.moveItem(at: disks, to: external)
        try f.fm.createSymbolicLink(at: disks, withDestinationURL: external)
        try f.checkCopy(f.config(), allowed: false)
        XCTAssertEqual(try f.fm.destinationOfSymbolicLink(atPath: disks.path), external.path)
        XCTAssertEqual(try Data(contentsOf: external.appendingPathComponent("hvf-target.raw")), f.bytes)
    }
    func testBundleSymlinkRefusesBeforeReservation() throws {
        let f = try HvfCloneSourceFixture()
        var config = f.config()
        config.bundlePath = try f.alias("bundle-alias", to: f.bundle).path
        try f.checkCopy(config, allowed: false)
        XCTAssertEqual(try Data(contentsOf: f.disk), f.bytes)
    }
    func testMissingDiskRetainsUIAdmissionButCopierRefusesBeforeReservation() throws {
        let f = try HvfCloneSourceFixture()
        try f.fm.removeItem(at: f.disk)
        XCTAssertNil(HvfWindowsCloneSource.refusal(for: f.config(), phase: .admission))
        try f.checkCopy(f.config(), allowed: false)
    }
    func testMissingBundleOrDisksRefusesBeforeReservation() throws {
        for missingBundle in [false, true] {
            let f = try HvfCloneSourceFixture()
            try f.fm.removeItem(at: missingBundle ? f.bundle : f.disk.deletingLastPathComponent())
            XCTAssertNil(HvfWindowsCloneSource.refusal(for: f.config(), phase: .admission))
            try f.checkCopy(f.config(), allowed: false)
        }
    }
    func testWrongTypeRefusesBothAdmissionAndCopy() throws {
        let f = try HvfCloneSourceFixture()
        try f.fm.removeItem(at: f.disk)
        try f.fm.createDirectory(at: f.disk, withIntermediateDirectories: false)
        XCTAssertNotNil(HvfWindowsCloneSource.refusal(for: f.config(), phase: .admission))
        try f.checkCopy(f.config(), allowed: false)
    }
    func testCyclicAncestorMetadataErrorRefusesBothAdmissionAndCopy() throws {
        let f = try HvfCloneSourceFixture()
        let cyclic = try f.alias("cyclic", to: f.root.appendingPathComponent("cyclic"))
        var config = f.config()
        config.bundlePath = cyclic.appendingPathComponent("bundle").path
        XCTAssertNotNil(HvfWindowsCloneSource.refusal(for: config, phase: .admission))
        try f.checkCopy(config, allowed: false)
        XCTAssertEqual(try Data(contentsOf: f.disk), f.bytes)
    }
}
