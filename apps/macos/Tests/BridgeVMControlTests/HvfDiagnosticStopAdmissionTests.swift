import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeDiagnosticStopAdmissionTests: XCTestCase {
    private func runnerArgs(_ config: HvfEngineConfig, owned: Bool) -> [String] {
        config.runnerArguments(manifestPath: "/test/launch.json", runnerPath: "/test/hvf-runner",
            firmwareCodePath: "/test/code.fd", probePath: "/test/probe", ownedRuntime: owned)
    }

    func testOnlyValidatedTwoOptionE2EAdmissionEnablesOwnedStop() throws {
        let lane = URL(fileURLWithPath: "/private/tmp", isDirectory: true).appendingPathComponent("bridgevm-e2e-stop-admission-\(UUID().uuidString)")
        let libraryRoot = lane.appendingPathComponent("library", isDirectory: true)
        let answer = lane.appendingPathComponent("e2e-unattend.xml")
        try FileManager.default.createDirectory(at: libraryRoot, withIntermediateDirectories: true)
        try Data("<unattend/>".utf8).write(to: answer)
        defer { try? FileManager.default.removeItem(at: lane) }

        let rootOnly = try BridgeVMControlLaunchOptions.parse(arguments: ["--e2e-library-root", libraryRoot.path])
        XCTAssertFalse(rootOnly.admitsHostDiagnosticStop)
        let admitted = try BridgeVMControlLaunchOptions.parse(arguments: [
            "--e2e-library-root", libraryRoot.path, "--e2e-unattend-path", answer.path])
        XCTAssertTrue(admitted.admitsHostDiagnosticStop)

        let saved = VMConfig(id: "windows", name: "windows", displayName: "windows",
            backendKind: "hvf-engine", bootMode: "windows-hvf",
            bundlePath: libraryRoot.appendingPathComponent("windows/bundle.vmbridge").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: "windows", displayWidth: 1280, displayHeight: 720,
            installPending: false)
        XCTAssertTrue(VMLibrary.save(saved, rootURL: libraryRoot))
        let ordinary = LibraryModel(rootURL: libraryRoot, migrateLegacy: false,
            startsModelsAutomatically: false)
        let e2e = LibraryModel(rootURL: libraryRoot,
            e2eUnattendedPath: admitted.e2eUnattendedPath?.path,
            e2eHostDiagnosticStopAdmitted: admitted.admitsHostDiagnosticStop,
            migrateLegacy: false, startsModelsAutomatically: false)
        let ordinaryConfig = try XCTUnwrap(ordinary.hvfRuntimeSession(for: saved)?.config)
        let e2eConfig = try XCTUnwrap(e2e.hvfRuntimeSession(for: saved)?.config)
        XCTAssertFalse(runnerArgs(ordinaryConfig, owned: true).contains("--helper-host-diagnostic-stop"))
        XCTAssertTrue(runnerArgs(e2eConfig, owned: true).contains("--helper-host-diagnostic-stop"))
        XCTAssertFalse(runnerArgs(e2eConfig, owned: false).contains("--helper-host-diagnostic-stop"))
        XCTAssertFalse(runnerArgs(try XCTUnwrap(HvfEngineConfig.libraryVM(saved, rootURL: libraryRoot)),
            owned: true).contains("--helper-host-diagnostic-stop"),
            "a matching /tmp library path alone is not admission")
    }

    func testPreparationRemovesStaleDiagnosticRequestSymlink() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("stop-preparation-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let vm = VMConfig(id: "windows", name: "windows", displayName: "windows",
            backendKind: "hvf-engine", bootMode: "windows-hvf",
            bundlePath: root.appendingPathComponent("bundle.vmbridge").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: "windows", displayWidth: 1280, displayHeight: 720,
            installPending: false)
        let config = try XCTUnwrap(HvfEngineConfig.libraryVM(vm, rootURL: root))
        let evidence = URL(fileURLWithPath: config.evidenceDir)
        try FileManager.default.createDirectory(at: evidence, withIntermediateDirectories: true)
        let request = evidence.appendingPathComponent("diagnostic-stop.request")
        let pending = evidence.appendingPathComponent("diagnostic-stop.request.pending")
        try FileManager.default.createSymbolicLink(at: request, withDestinationURL: root.appendingPathComponent("missing"))
        try FileManager.default.createSymbolicLink(at: pending, withDestinationURL: root.appendingPathComponent("missing"))
        _ = try HvfRuntimePreparation.prepare(config: config)
        XCTAssertThrowsError(try FileManager.default.destinationOfSymbolicLink(atPath: request.path))
        XCTAssertThrowsError(try FileManager.default.destinationOfSymbolicLink(atPath: pending.path))
    }
}
