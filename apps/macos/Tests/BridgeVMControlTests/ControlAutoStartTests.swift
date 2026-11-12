import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class ControlAutoStartTests: XCTestCase {
    func testOnlyStoppedStartableVMIsEligible() throws {
        let f = try HvfOwnedRuntimeFixture(); defer { f.clean() }
        let root = f.root.appendingPathComponent("library")
        var saved = VMConfig(id: "windows", name: "Windows", displayName: "Windows", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: f.root.appendingPathComponent("bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: "windows", displayWidth: 1280, displayHeight: 800)
        saved.startWithBridgeVM = true
        let session = f.session()
        let library = LibraryModel(rootURL: root, migrateLegacy: false, runtimeSessionFactory: { _ in session }, startsModelsAutomatically: false)
        library.vms = [saved]
        XCTAssertTrue(ControlAutoStart.isStartable(saved, library: library), "Stopped and ready VM starts")
        var pending = saved; pending.installPending = true
        XCTAssertFalse(ControlAutoStart.isStartable(pending, library: library), "Install pending is skipped")
        let transaction = HvfWindowsInstallFinalization.paths(slug: saved.slug, libraryRoot: root, bundlePath: saved.bundlePath).transaction
        try FileManager.default.createDirectory(at: transaction, withIntermediateDirectories: true)
        XCTAssertFalse(ControlAutoStart.isStartable(saved, library: library), "Interrupted installation is skipped")
        try FileManager.default.removeItem(at: transaction)
        try FileManager.default.removeItem(atPath: session.config.targetDiskPath)
        XCTAssertFalse(ControlAutoStart.isStartable(saved, library: library), "Missing disk is skipped")
        try Data([0]).write(to: URL(fileURLWithPath: session.config.targetDiskPath))
        f.existingRuntime = true
        XCTAssertFalse(ControlAutoStart.isStartable(saved, library: library), "Surviving runner is skipped")
        f.existingRuntime = false
        session.connectionState = .booting
        XCTAssertFalse(ControlAutoStart.isStartable(saved, library: library), "Active session is skipped")
        session.connectionState = .stopped
    }
    func testSelectionAndOptionKeySkip() throws {
        let f = try LibraryDashboardNavigationFixture(); defer { f.clean() }
        var on = try XCTUnwrap(f.library.vms.first), off = on, installing = on
        on.startWithBridgeVM = true
        off.startWithBridgeVM = false
        installing.startWithBridgeVM = true; installing.installPending = true
        XCTAssertEqual(ControlAutoStart.selection([off, on, installing], optionHeld: false), [on])
        XCTAssertEqual(ControlAutoStart.selection([on], optionHeld: true), [])
    }
    func testSequentialOrderingAndFailureDoesNotSkipNextVM() async throws {
        let f = try LibraryDashboardNavigationFixture(); defer { f.clean() }
        var first = try XCTUnwrap(f.library.vms.first), second = first
        first.startWithBridgeVM = true; second.startWithBridgeVM = true
        first.id = "first"; second.id = "second"
        var events: [String] = []
        await ControlAutoStart.run([first, second], optionHeld: false, start: { config in
            events.append("begin-" + config.slug)
            await Task.yield()
            if config.slug == "first" { throw CocoaError(.fileReadUnknown) }
            events.append("end-" + config.slug)
        }, failed: { config, _ in events.append("failed-" + config.slug) })
        XCTAssertEqual(events, ["begin-first", "failed-first", "begin-second", "end-second"])
    }
    func testRegistrationRoundTripAndMissingAndUnknownFields() throws {
        let f = try LibraryDashboardNavigationFixture(); defer { f.clean() }
        var config = try XCTUnwrap(f.library.vms.first)
        var object = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(config)) as? [String: Any])
        object["futurePreference"] = true
        XCTAssertNil(try JSONDecoder().decode(VMConfig.self, from: JSONSerialization.data(withJSONObject: object)).startWithBridgeVM)
        config.startWithBridgeVM = true
        XCTAssertEqual(try JSONDecoder().decode(VMConfig.self, from: JSONEncoder().encode(config)).startWithBridgeVM, true)
    }
}
