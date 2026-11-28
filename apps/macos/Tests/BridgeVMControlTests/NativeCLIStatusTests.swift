import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeCLIStatusTests: XCTestCase {
    func testStatusOptionsPreserveUnicodeWithoutRequiringRegistration() throws {
        for arguments in [["status", "개발-vm", "--json"], ["--json", "status", "개발-vm"]] {
            let options = try NativeCLIOptions.parse(arguments: arguments)
            XCTAssertEqual(options.command, .status("개발-vm"))
            XCTAssertTrue(options.json)
        }
        XCTAssertTrue(try NativeCLIOptions.parse(arguments: ["status", "--help"]).showHelp)
        for arguments in [["status"], ["status", "../vm"], ["status", "vm", "extra"]] {
            XCTAssertThrowsError(try NativeCLIOptions.parse(arguments: arguments))
        }
    }

    func testAbsentOwnerReturnsStructuredUnobservedWithoutCreatingLibrary() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("native-status-\(UUID().uuidString)")
        let snapshot = NativeCLIRuntimeStatus.snapshot(rootURL: root, id: "missing")
        XCTAssertFalse(snapshot.complete)
        XCTAssertEqual(snapshot.runtimeState, "unobserved")
        XCTAssertEqual(snapshot.unavailableReason, "ownerUnavailable")
        XCTAssertTrue(snapshot.sessions.isEmpty)
        XCTAssertNil(snapshot.appInstanceID)
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.path))
    }

    func testMissingAndUnreadableSavedConfigurationDoNotMutateLibrary() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("native-status-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let handle = try NativeRuntimeLibraryHandle.open(rootURL: root, create: false)
        XCTAssertEqual(NativeCLIStatusConfiguration.read(library: handle, id: "missing").state, .missing)
        let entry = root.appendingPathComponent("broken")
        try FileManager.default.createDirectory(at: entry, withIntermediateDirectories: true)
        let bytes = Data("invalid config".utf8), config = entry.appendingPathComponent("vm.json")
        try bytes.write(to: config)
        let saved = NativeCLIStatusConfiguration.read(library: handle, id: "broken")
        XCTAssertEqual(saved.state, .unreadable)
        XCTAssertNil(saved.digest)
        XCTAssertEqual(try Data(contentsOf: config), bytes)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), ["broken"])
    }

    /// Text uses the dashboard's state words; PIDs, run tokens, ownership codes and the scope stay in JSON.
    func testStatusTextIsPlainAndJSONKeepsProcessIdentity() throws {
        let token = NativeRuntimeControlTestSupport.token
        let running = NativeRuntimeSessionObservation(ownership: .owned, connectionState: "connected",
            acceptedConfigurationDigest: nil, configurationMatch: .different,
            ownedProcess: .init(token: token, processID: 123),
            lastOwnedExit: .init(process: .init(token: token, processID: 77), reason: "uncaughtSignal", status: 9),
            graphicsMode: .basic3DOff)
        let attached = NativeRuntimeSessionObservation(ownership: .attachedObservation, connectionState: "stopped",
            acceptedConfigurationDigest: nil, configurationMatch: .unknown, ownedProcess: nil, lastOwnedExit: nil,
            graphicsMode: .unverified)
        func status(_ sessions: [NativeRuntimeSessionObservation]) -> NativeCLIRuntimeStatus {
            NativeCLIRuntimeStatus(vmID: "vm", libraryPath: "/l", savedConfiguration: .init(state: .missing, digest: nil),
                complete: true, runtimeState: sessions.isEmpty ? "unobserved" : "observed", unavailableReason: nil,
                appInstanceID: NativeRuntimeControlTestSupport.app, observedAt: 1, sessions: sessions)
        }
        let observed = status([running, attached])
        XCTAssertEqual(observed.text, """
        VM status: vm
        No saved VM has this ID.
        State: Running
          Its saved settings changed after it started. Restart the VM to use them.
          Graphics: standard display, 3D off
          Last run ended by signal 9.
        State: Running
          Started by another app or tool. Stop it there.

        """)
        let json = String(decoding: try JSONEncoder().encode(observed), as: UTF8.self)
        for value in [token, "\"processID\":123", "attached-observation", NativeRuntimeCodec.scope] {
            XCTAssertTrue(json.contains(value), value)
        }
        XCTAssertEqual(status([]).text, "VM status: vm\nNo saved VM has this ID.\nBridgeVM isn’t running this VM.\n")
    }
}
