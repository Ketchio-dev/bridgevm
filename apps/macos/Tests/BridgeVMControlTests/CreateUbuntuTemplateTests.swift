import SwiftUI
import XCTest
@testable import BridgeVMControl

@MainActor
final class CreateUbuntuTemplateTests: XCTestCase {
    func testIncompatibleOnlyLibraryDoesNotOfferUbuntuCloneOrReachCopy() throws {
        for (engine, boot) in [("hvf-engine", "windows-hvf"), ("qemu-compat", "windows-iso"), ("fast-vz", "iso-efi")] {
            let f = try VMCloneDestinationFixture()
            var config = f.config(windows: true)
            config.backendKind = engine; config.bootMode = boot
            let sheet = sheet(f, configs: [config])
            let footer = try footer(sheet)
            XCTAssertNil(sheet.template, engine)
            XCTAssertFalse(footer.canCreate, engine)
            XCTAssertEqual(footer.failureCode, "template-unavailable", engine)
            var reachedCopy = false
            // Exercise the selected config through the real factory, with an owned root.
            // The UI transaction's global-root defaults are deliberately not invoked.
            let clone = sheet.template.flatMap {
                VMLibrary.cloneUbuntu(name: "Copy", template: $0,
                    storageDir: f.root.appendingPathComponent("output"), libraryRoot: f.library,
                    beforeCopy: { reachedCopy = true; return false })
            }
            XCTAssertNil(clone)
            XCTAssertFalse(reachedCopy, "\(engine) reached reservation and the factory's copy boundary")
            XCTAssertFalse(f.fm.fileExists(atPath: f.root.appendingPathComponent("output/copy").path))
            XCTAssertEqual(try Data(contentsOf: f.sentinel), Data("keep source unchanged".utf8))
        }
    }

    func testDirectKernelTemplateWinsOverAnEarlierWindowsEntry() throws {
        let f = try VMCloneDestinationFixture()
        var windows = f.config(windows: true); windows.id = "windows"
        var ubuntu = f.config(windows: false); ubuntu.bootMode = "direct-kernel"
        for mode: CreateVMSheet.Mode in [.ubuntu, .iso, .windows] {
            let selected = sheet(f, configs: [windows, ubuntu], mode: mode)
            XCTAssertEqual(selected.template, ubuntu)
            XCTAssertTrue(try footer(selected).canCreate)
            XCTAssertEqual(try footer(selected).failureCode, "")
        }
    }

    func testLegacyNilBootModeStillCopiesAndRegistersTinyTemplate() throws {
        let f = try VMCloneDestinationFixture()
        let metadata = f.source.appendingPathComponent("metadata")
        XCTAssertEqual(try Data(contentsOf: metadata.appendingPathComponent("hvf-vars.fd")), Data("owned clone vars".utf8))
        for file in ["apple-vz-launch.json", "handoff.json"] {
            XCTAssertTrue(JSONFile.writeDict(["vm_name": "Source", "bundle_path": f.source.path],
                to: metadata.appendingPathComponent(file).path))
        }
        let source = f.config(windows: false)
        let selected = sheet(f, configs: [source])
        XCTAssertNil(source.bootMode)
        XCTAssertEqual(selected.template, source)
        XCTAssertTrue(try footer(selected).canCreate)
        let clone = try XCTUnwrap(VMLibrary.cloneUbuntu(name: "Copy", template: try XCTUnwrap(selected.template),
            storageDir: f.root.appendingPathComponent("output"), libraryRoot: f.library))
        XCTAssertEqual(clone.backendKind, "fast-vz")
        XCTAssertEqual(clone.bootMode, "direct-kernel")
        XCTAssertEqual(VMLibrary.list(rootURL: f.library).first, clone)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: clone.bundlePath).appendingPathComponent("sentinel")),
            Data("keep source unchanged".utf8))
        XCTAssertEqual(try Data(contentsOf: f.sentinel), Data("keep source unchanged".utf8))
    }

    func testISOAndQemuModesRetainTheirExistingFallback() throws {
        let f = try VMCloneDestinationFixture()
        let other = f.config(windows: true)
        for mode: CreateVMSheet.Mode in [.iso, .windows] {
            let selected = sheet(f, configs: [other], mode: mode)
            XCTAssertEqual(selected.template, other)
            XCTAssertTrue(try footer(selected).canCreate)
            XCTAssertEqual(try footer(selected).failureCode, "")
        }
    }

    func testAddingAndRemovingEligibleTemplateUpdatesSameSheet() throws {
        let f = try VMCloneDestinationFixture()
        var other = f.config(windows: true); other.id = "windows"
        let valid = f.config(windows: false)
        let selected = sheet(f, configs: [other])
        XCTAssertFalse(try footer(selected).canCreate)
        selected.library.vms.append(valid)
        XCTAssertEqual(selected.template, valid)
        XCTAssertTrue(try footer(selected).canCreate)
        XCTAssertEqual(try footer(selected).failureCode, "")
        selected.library.vms = [other]
        XCTAssertNil(selected.template)
        XCTAssertFalse(try footer(selected).canCreate)
        XCTAssertEqual(try footer(selected).failureCode, "template-unavailable")
    }

    private func sheet(_ f: VMCloneDestinationFixture, configs: [VMConfig],
        mode: CreateVMSheet.Mode = .ubuntu) -> CreateVMSheet {
        let library = LibraryModel(rootURL: f.root.appendingPathComponent("ui-library"),
            migrateLegacy: false, startsModelsAutomatically: false)
        library.vms = configs
        return CreateVMSheet(library: library, osFamily: mode.family, mode: mode, name: "Copy", isoPath: "/owned-unused.iso")
    }

    private func footer(_ sheet: CreateVMSheet) throws -> CreateVMCreationFooter {
        try XCTUnwrap(values(CreateVMCreationFooter.self, in: sheet.body).first)
    }

    private func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let result = value as? T { return [result] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }
}
