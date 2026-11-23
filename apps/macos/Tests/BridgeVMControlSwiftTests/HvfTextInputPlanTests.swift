// sources: apps/macos/Sources/BridgeVMControl/HvfEngine/HvfTextInputPlan.swift
import Foundation
import Testing



/// A USB HID keyboard carries physical key usages, so non-ASCII can never go
/// down that path. These pin which transport each input takes; the last test is
/// the negative control for the bug this replaced.
@Suite("HvfTextInputPlan")
struct HvfTextInputPlanTests {
    @Test("printable ASCII goes over HID and does not touch the clipboard")
    func asciiUsesHid() {
        let plan = HvfTextInputPlan.make(for: "hello")
        #expect(plan.hidChunks == ["68656c6c6f"])
        #expect(plan.clipboardBase64 == nil)
    }

    @Test("Korean goes over the clipboard and survives the round trip")
    func koreanUsesClipboard() throws {
        let plan = HvfTextInputPlan.make(for: "한글")
        #expect(plan.hidChunks.isEmpty)
        let encoded = try #require(plan.clipboardBase64)
        let decoded = try #require(Data(base64Encoded: encoded))
        #expect(String(data: decoded, encoding: .utf8) == "한글")
    }

    /// Mixed content must not be split across two transports: the guest could
    /// reorder them, and a scrambled string is worse than a slow one.
    @Test("mixed content goes entirely over the clipboard")
    func mixedUsesClipboardOnly() throws {
        let plan = HvfTextInputPlan.make(for: "abc한글")
        #expect(plan.hidChunks.isEmpty)
        let encoded = try #require(plan.clipboardBase64)
        let decoded = try #require(Data(base64Encoded: encoded))
        #expect(String(data: decoded, encoding: .utf8) == "abc한글")
    }

    @Test("long ASCII splits at the guest's 32-byte token cap")
    func longAsciiSplits() {
        let plan = HvfTextInputPlan.make(for: String(repeating: "x", count: 70))
        #expect(plan.hidChunks.count == 3)
        #expect(plan.hidChunks.allSatisfy { $0.count <= 64 })
    }

    @Test("empty input produces nothing")
    func emptyProducesNothing() {
        let plan = HvfTextInputPlan.make(for: "")
        #expect(plan.hidChunks.isEmpty)
        #expect(plan.clipboardBase64 == nil)
    }

    /// Negative control. The previous implementation filtered to 0x20...0x7e,
    /// so Korean was dropped with no diagnostic. If this ever stops holding,
    /// the ASCII-only assumption has changed and the routing above is moot.
    @Test("the old ASCII filter would have dropped Korean entirely")
    func oldFilterDroppedKorean() {
        #expect("한글".utf8.filter { (0x20...0x7e).contains($0) }.isEmpty)
    }
}
