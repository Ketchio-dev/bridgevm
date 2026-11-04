// sources: apps/macos/Sources/BridgeVMControl/HvfEngine/HvfAppPacingRecords.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfDisplayPacingRecord.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfMainThreadStalls.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfOrderedInputRecord.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfSessionInputDriver.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfTextInputTargetIdentity.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfSessionInputRouter.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfSessionInputGeneration.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfOrderedInputQueue.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfOrderedInputEvent.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfRecoverableInputStream.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfNegotiatedInputStream.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfAcknowledgedInputStream.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfInputCapabilitiesRequest.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfInputCapabilityReceipt.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfUnicodeInputRequest.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfGuestInputEncoding.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfControlChordKeys.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfPointerInputEncoding.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfPointerScrollEncoding.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfPointerHeldState.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfPointerCleanup.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfInputReceiptEnvelope.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfGuestResetRecord.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfSessionInputDriverObservation.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfModifierChord.swift
import Foundation
import Testing

/// Binding changes discard the queued generation through each driver entry point. The observer must
/// account for the awaited event and its successor without changing the router's fail-closed refusal.
@MainActor
@Suite("HvfInputBindingDiscardRecord", .serialized)
struct HvfInputBindingDiscardRecordTests {
    static let binding = ["disk", "vars", "evidence", "control"]

    @Test("binding changes count every discarded event exactly once", arguments: ["route", "poll", "legacy"])
    func bindingDiscards(entry: String) throws {
        let driver = HvfSessionInputDriver()
        var input = HvfOrderedInputRecord()
        var commands: [String] = []
        driver.onAdmission = { input.admitted($0, held: $1, depth: $2) }
        driver.onUpdate = { input.note($0, now: 1_000_000) }
        func poll(_ lines: [String] = [], binding: [String]? = nil) {
            driver.poll(binding: binding ?? Self.binding, serviceReady: true, lines: lines) {
                commands.append($0); return true
            }
        }
        driver.beginOwnedBoot(binding: Self.binding)
        poll()
        let capability = try #require(commands.last)
        let capabilityID = try #require(capability.split(separator: " ").dropFirst().first)
        poll(["BVAGENT CMD \(capability) exit=0",
              "BVINPUT_CAPS \(capabilityID) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536", "BVAGENT END \(capability)"])
        poll()
        #expect(driver.router.state == .ready)
        #expect(driver.route(.text("a"), binding: Self.binding) == .queued)
        poll()
        #expect(driver.route(.text("b"), binding: Self.binding) == .queued)
        #expect(driver.router.count == 2 && input.sent == 1)
        let before = commands.count
        let other = ["another-disk", "vars", "evidence", "control"]
        func changeBinding() {
            switch entry {
            case "route": #expect(driver.route(.text("c"), binding: other) == .refused)
            case "poll": poll(binding: other)
            default: #expect(!driver.allowLegacyWrite(binding: other))
            }
        }
        changeBinding()
        #expect(driver.router.failure == .bindingChanged && driver.router.count == 0)
        #expect(input.discarded == 2 && input.inserted == 0)
        #expect(input.sent == 1 && commands.count == before, "refused ownership never sends another event")
        changeBinding()
        #expect(input.discarded == 2, "a later refusal cannot count the same events again")
    }
}
