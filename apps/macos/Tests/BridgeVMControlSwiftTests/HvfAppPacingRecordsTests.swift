// sources: apps/macos/Sources/BridgeVMControl/HvfEngine/HvfAppPacingRecords.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfDisplayPacingRecord.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfMainThreadStalls.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfOrderedInputRecord.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfSessionInputDriver.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfTextInputTargetIdentity.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfSessionInputRouter.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfSessionInputGeneration.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfOrderedInputQueue.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfOrderedInputEvent.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfRecoverableInputStream.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfNegotiatedInputStream.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfAcknowledgedInputStream.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfInputCapabilitiesRequest.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfInputCapabilityReceipt.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfUnicodeInputRequest.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfGuestInputEncoding.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfPointerInputEncoding.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfPointerScrollEncoding.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfPointerHeldState.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfPointerCleanup.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfInputReceiptEnvelope.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfGuestResetRecord.swift
import Foundation
import Testing

/// A runtime session's app-side pacing records: the ordered input route's record as the real input
/// driver feeds it, and the session boundaries that decide what is counted and written.
@MainActor
@Suite("HvfAppPacingRecords", .serialized)
struct HvfAppPacingRecordsTests {
    static let binding = ["disk", "vars", "evidence", "control"]
    static let ms: UInt64 = 1_000_000

    /// A driver whose ordered stream negotiated INPUTCAPS, observed by an open session's records.
    @MainActor final class Route {
        let records = HvfAppPacingRecords()
        let driver = HvfSessionInputDriver()
        var now: UInt64 = 0
        var commands: [String] = []

        init() {
            records.clock = { [unowned self] in self.now }
            records.begin()
            records.observe(driver)
            driver.beginOwnedBoot(binding: HvfAppPacingRecordsTests.binding)
            poll()
            let command = commands.removeLast(), id = command.split(separator: " ")[1]
            poll(["BVAGENT CMD \(command) exit=0",
                  "BVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536", "BVAGENT END \(command)"])
            poll()
            #expect(driver.router.state == .ready)
        }

        func poll(_ lines: [String] = []) {
            driver.poll(binding: HvfAppPacingRecordsTests.binding, serviceReady: true, lines: lines) {
                self.commands.append($0); return true
            }
        }

        func route(_ event: HvfOrderedInputQueue.Event) -> HvfNegotiatedInputStream.Admission {
            driver.route(event, binding: HvfAppPacingRecordsTests.binding)
        }

        /// The agent's receipt for the sent `command`: inserted, or failed without insertion.
        func receipt(_ command: String, inserted: Bool = true) -> [String] {
            let fields = command.split(separator: " "), label = fields.prefix(2).joined(separator: " ")
            return ["BVAGENT CMD \(label) exit=\(inserted ? 0 : 1)"]
                + (inserted ? ["BVINPUT_INSERTED \(fields[1]) 1"] : []) + ["BVAGENT END \(label)"]
        }
    }

    static func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("bridgevm-app-pacing-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    @Test("depth, coalescing and round trips come from the driver's admissions and updates")
    func orderedRoundTrips() throws {
        let route = Route()
        #expect(route.route(.pointer("move:1x2")) == .queued)
        route.now = 1_000 * Self.ms
        route.poll()                                   // sends the first move
        let first = try #require(route.commands.last)
        #expect(route.route(.pointer("move:3x4")) == .queued)  // behind the awaited move: depth 2
        #expect(route.route(.pointer("move:5x6")) == .queued)  // replaces it in place: still 2
        route.now += 15 * Self.ms
        route.poll(route.receipt(first))               // inserted after 15 ms; the next move is sent
        let second = try #require(route.commands.last)
        #expect(second != first)
        route.now += 40 * Self.ms
        route.poll(route.receipt(second))              // inserted after 40 ms
        #expect(route.records.input.line() == "app ordered input: queued=3 coalesced=1 legacy=0 refused=0 "
                + "sent=2 inserted=2 discarded=0 depth_1=1 depth_2=2 depth_le_4=0 depth_le_16=0 depth_gt_16=0 "
                + "max_depth=2 rtt_le_10ms=0 rtt_le_20ms=1 rtt_le_50ms=1 rtt_le_100ms=0 rtt_le_250ms=0 "
                + "rtt_gt_250ms=0 max_rtt_ms=40")
    }

    @Test("legacy and refused admissions, discarded events and a failed receipt count without a round trip")
    func admissionsAndCancellations() throws {
        let route = Route()
        #expect(route.route(.text("a")) == .queued)
        route.poll()
        #expect(route.route(.text("b")) == .queued)
        route.driver.cancelTarget()                    // drops both: the awaited and the queued text
        #expect(route.records.input.discarded == 2 && route.records.input.inserted == 0)
        let attached = HvfSessionInputDriver()
        route.records.observe(attached)
        attached.attachUnknown(binding: Self.binding)
        #expect(attached.route(.key("c"), binding: Self.binding) == .legacy)
        #expect(attached.route(.key("d"), binding: ["other"]) == .refused)
        let input = route.records.input
        #expect([input.queued, input.legacy, input.refused, input.sent, input.inserted] == [2, 1, 1, 1, 0])
        #expect(input.rtts.reduce(0, +) == 0 && input.maxRtt == 0)
        // The agent ran the request without inserting it: the stream cancels it, with no round trip.
        let failing = Route()
        #expect(failing.route(.text("e")) == .queued)
        failing.poll()
        failing.now = 5 * Self.ms
        failing.poll(failing.receipt(try #require(failing.commands.last), inserted: false))
        let failed = failing.records.input
        #expect([failed.sent, failed.inserted, failed.discarded, failed.maxRtt] == [1, 0, 1, 0])
    }

    @Test("only an open session counts, and it writes its two lines once as it ends")
    func sessionBoundaries() throws {
        let records = HvfAppPacingRecords()
        let directory = try Self.directory()
        defer { try? FileManager.default.removeItem(at: directory) }
        final class Link {}
        let link = ObjectIdentifier(Link.self)
        records.display(.repeated, timestamp: 1, target: 1.015625, link: link, work: 0)
        #expect(records.display.ticks == 0 && !records.stalls.running, "no session is open")
        #expect(!records.end(evidenceDir: directory.path))
        records.begin()
        #expect(!records.end(evidenceDir: directory.path), "a session that recorded nothing writes nothing")
        let file = directory.appendingPathComponent("app-pacing.log")
        #expect(!FileManager.default.fileExists(atPath: file.path))
        records.begin()
        records.display(.repeated, timestamp: 1, target: 1.015625, link: link, work: 0)
        #expect(records.stalls.running, "the session's first tick starts the stall observer")
        #expect(records.end(evidenceDir: directory.path))
        #expect(!records.stalls.running)
        let written = try String(contentsOf: file, encoding: .utf8)
        #expect(written == records.lines.map { $0 + "\n" }.joined())
        #expect(written.hasPrefix("app display pacing: ticks=1 presented=0 repeated=1 "))
        #expect(written.split(separator: "\n").map { String($0.prefix(18)) } == ["app display pacing", "app ordered input:"])
        #expect(!records.end(evidenceDir: directory.path), "a session ends once")
        records.display(.repeated, timestamp: 2, target: 2.015625, link: link, work: 0)
        #expect(records.display.ticks == 1, "ticks after the end are not counted")
        records.begin()
        #expect(records.display == HvfDisplayPacingRecord() && records.input.line() == HvfOrderedInputRecord().line())
    }

    @Test("the file is replaced by rename: never written through a link, never into a missing directory")
    func fileSafety() throws {
        let directory = try Self.directory()
        defer { try? FileManager.default.removeItem(at: directory) }
        let outside = directory.appendingPathComponent("outside.txt")
        try Data("kept\n".utf8).write(to: outside)
        let file = directory.appendingPathComponent("app-pacing.log")
        try FileManager.default.createSymbolicLink(at: file, withDestinationURL: outside)
        #expect(HvfAppPacingFile.write(["a", "b"], evidenceDir: directory.path))
        #expect(try String(contentsOf: outside, encoding: .utf8) == "kept\n")
        let values = try file.resourceValues(forKeys: [.isSymbolicLinkKey, .isRegularFileKey])
        #expect(values.isSymbolicLink != true && values.isRegularFile == true)
        #expect(try String(contentsOf: file, encoding: .utf8) == "a\nb\n")
        #expect(!HvfAppPacingFile.write(["a"], evidenceDir: directory.appendingPathComponent("missing").path))
        #expect(!HvfAppPacingFile.write(["a"], evidenceDir: outside.path), "a file is not a directory")
        #expect(!HvfAppPacingFile.write(["a"], evidenceDir: ""))
        #expect(!FileManager.default.fileExists(atPath: directory.appendingPathComponent("missing").path))
    }
}
