import XCTest
@testable import BridgeVMControl

final class LibraryCacheInventoryTests: XCTestCase {
    func testCacheNameDoesNotHideRegistrationsAliasesOrOtherInvalidEntries() throws {
        for kind in ["record", "record-link", "alias", "other", "lowercase"] {
            let fm = FileManager.default
            let root = fm.temporaryDirectory.resolvingSymlinksInPath()
                .appendingPathComponent("cache-inventory-negative-" + UUID().uuidString)
            try fm.createDirectory(at: root, withIntermediateDirectories: false)
            defer { try? fm.removeItem(at: root) }
            let name = kind == "other" ? "Not Canonical" : kind == "lowercase" ? "derived" : "Derived"
            let entry = root.appendingPathComponent(name)
            if kind == "alias" {
                try fm.createSymbolicLink(at: entry, withDestinationURL: root.appendingPathComponent("missing"))
            } else {
                try fm.createDirectory(at: entry, withIntermediateDirectories: false)
                if kind == "record" { try Data("{}".utf8).write(to: entry.appendingPathComponent("vm.json")) }
                if kind == "record-link" {
                    try fm.createSymbolicLink(at: entry.appendingPathComponent("vm.json"),
                        withDestinationURL: root.appendingPathComponent("missing"))
                }
            }
            XCTAssertFalse(VMLibraryInternalEntries.isSourceCache(entry))
            XCTAssertEqual(VMLibrary.scan(rootURL: root).issues.count, 1)
            XCTAssertEqual(try NativeLibraryReader.snapshot(rootURL: root).issues.count, 1)
        }
    }

    func testOwnSourceCacheIsNotReportedAsAnUnsafeVMEntry() throws {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("cache-inventory-" + UUID().uuidString)
        try fm.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? fm.removeItem(at: root) }
        let plan = HvfWindowsInstallPlan(repoRoot: root, libraryRoot: root,
            bundlePath: root.appendingPathComponent("vm/bundle").path, slug: "vm",
            request: .init(isoPath: "/synthetic.iso", isoSHA256: String(repeating: "a", count: 64),
                           diskGiB: 64, injectViogpu3d: false))
        let owner = try HvfWindowsInstallSourceLock(sourceImagePath: plan.sourceImagePath)
        defer { withExtendedLifetime(owner) {} }
        let gui = VMLibrary.scan(rootURL: root)
        let cli = try NativeLibraryReader.snapshot(rootURL: root)
        XCTAssertTrue(gui.configs.isEmpty)
        XCTAssertTrue(cli.records.isEmpty)
        XCTAssertTrue(gui.issues.isEmpty, "Internal source cache is not a malformed VM: \(gui.issues)")
        XCTAssertTrue(cli.issues.isEmpty, "Internal source cache is not an unsafe VM: \(cli.issues)")
    }
}
