import Foundation
import XCTest
@testable import BridgeVMControl

private final class HvfKeyboardDraftAccess: VTPMStateKeyProviding {
    var processLookups = 0
    var keyRequests = 0

    func stateKey(for stableVMID: String, allowCreation: Bool) throws -> Data {
        keyRequests += 1
        XCTFail("Keyboard draft tests must not request a key")
        throw CocoaError(.fileReadUnknown)
    }
}

@MainActor
final class HvfRuntimeKeyboardDraftFixture {
    let root: URL
    let evidence: URL
    let control: URL
    let input: URL
    private let access = HvfKeyboardDraftAccess()

    init() throws {
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent("keyboard-draft-" + UUID().uuidString)
        evidence = root.appendingPathComponent("evidence")
        control = root.appendingPathComponent("share/agent.ctl")
        input = evidence.appendingPathComponent("input.ctl")
        try FileManager.default.createDirectory(at: evidence, withIntermediateDirectories: true)
    }

    func remove() { try? FileManager.default.removeItem(at: root) }

    // This synchronous scope never pumps the run loop. Release the actual
    // Session and its input-driver timer before the caller removes its files.
    func withSession(_ body: (HvfEngineSession) throws -> Void) throws {
        let config = HvfEngineConfig(
            targetDiskPath: root.appendingPathComponent("absent-disk.raw").path,
            uefiVarsPath: root.appendingPathComponent("absent-vars.fd").path,
            evidenceDir: evidence.path, watchdogMs: nil, ramMiB: 6144, smpCpus: 4,
            clipboardSync: false, shareHostDir: root.appendingPathComponent("share").path,
            shareGuestDir: "C:\\shared", virtioNet: false, audioEnabled: false,
            virtioGpu3d: false, nvmeBufferedIO: true, ctlFilePath: control.path,
            vtpmStateDir: root.appendingPathComponent("absent-vtpm").path,
            swtpmBin: root.appendingPathComponent("absent-swtpm").path,
            vtpmKeyID: "keyboard-draft-fixture", allowsExperimental3D: false)
        var session: HvfEngineSession? = HvfEngineSession(
            config: config, repoRoot: root.appendingPathComponent("absent-repo"),
            processIsRunning: { [access] _ in
                access.processLookups += 1
                XCTFail("Keyboard draft tests must not inspect or attach a process")
                return false
            }, vtpmKeyProvider: access)
        weak var releasedSession = session
        defer {
            session?.beginOwnedInputBoot() // Invalidates the input pump without I/O.
            session = nil
            XCTAssertNil(releasedSession, "Session must be released before fixture cleanup")
            XCTAssertEqual(access.processLookups, 0)
            XCTAssertEqual(access.keyRequests, 0)
            for name in ["absent-disk.raw", "absent-vars.fd", "absent-vtpm", "absent-swtpm", "absent-repo"] {
                XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent(name).path))
            }
        }
        try body(try XCTUnwrap(session))
    }

    func connect(_ session: HvfEngineSession) throws {
        // Synthetic local observations only: no Start, attachment or guest.
        try appendLog("BVAGENT READY host=draft-fixture t=1\nBVAGENT SERVICE start t=2\n")
        session.poll()
        XCTAssertEqual(session.connectionState, .connected(host: "draft-fixture"))
        XCTAssertTrue(session.events.contains(.serviceStart(tMs: 2)))
        XCTAssertNil(try bytes(control))
        XCTAssertNil(try bytes(input))
    }

    func negotiateOrderedInput(_ session: HvfEngineSession) throws {
        try connect(session)
        session.beginOwnedInputBoot()
        session.poll()
        let command = try XCTUnwrap(try commands().first)
        let words = command.split(separator: " ")
        XCTAssertEqual(words.count, 2)
        XCTAssertEqual(words.first, "INPUTCAPS")
        guard words.count == 2, words.first == "INPUTCAPS",
              UUID(uuidString: String(words[1])) != nil else {
            throw CocoaError(.fileReadCorruptFile)
        }
        try appendLog("BVAGENT CMD \(command) exit=0\n"
            + "BVINPUT_CAPS \(words[1]) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536\n"
            + "BVAGENT END \(command)\n")
        session.poll()
        session.poll()
        let ready = session.events.contains(.unknown(
            "ordered input active: receipts confirm insertion, not application consumption"))
        XCTAssertTrue(ready, "Owned capability frame must activate the actual input router")
        guard ready else { throw CocoaError(.fileReadCorruptFile) }
        XCTAssertEqual(try commands(), [command])
        XCTAssertNil(try bytes(input))
    }

    func commands() throws -> [String] {
        guard let data = try bytes(control) else { return [] }
        return String(decoding: data, as: UTF8.self).split(separator: "\n").map(String.init)
    }

    func bytes(_ url: URL) throws -> Data? {
        guard FileManager.default.fileExists(atPath: url.path) else { return nil }
        return try Data(contentsOf: url)
    }

    private func appendLog(_ text: String) throws {
        let log = evidence.appendingPathComponent("run.log")
        if !FileManager.default.fileExists(atPath: log.path) { try Data().write(to: log) }
        let handle = try FileHandle(forWritingTo: log)
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(text.utf8))
    }
}
