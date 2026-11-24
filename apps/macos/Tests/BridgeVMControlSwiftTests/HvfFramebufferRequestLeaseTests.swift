// sources: apps/macos/Sources/BridgeVMControl/HvfEngine/HvfFramebufferFileIdentity.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfFramebufferMappedRegion.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfFramebufferMapping.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfFramebufferRequestLease.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfFramebufferFrameRequester.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfFramebufferFrameReader.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfFramebufferHeaderFields.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfFramebufferDamage.swift
#if canImport(AppKit)
import Foundation
import Testing

@MainActor
@Suite("Framebuffer request backing identity")
struct HvfFramebufferRequestLeaseTests {
    @Test func replacementCannotAcquireLeaseForOldMapping() async throws {
        try await withFrameFile { root, path, original in
            let mapping = try #require(HvfFramebufferMapping(path: path.path))
            let saved = root.appendingPathComponent("mapped.fb")
            try FileManager.default.moveItem(at: path, to: saved)
            let replacement = Data(repeating: 0xcc, count: original.count)
            try replacement.write(to: path)
            let lease = HvfFramebufferRequestLease()
            #expect(lease.descriptor(for: mapping, path: path.path) == nil)
            let after = try Data(contentsOf: path)
            let mapped = try Data(contentsOf: saved)
            #expect(after == replacement, "a replacement received the old mapping's request")
            #expect(mapped == original)
        }
    }

    @Test func reusingRequesterForAnotherMappingPostsOnlyToThatBacking() async throws {
        try await withFrameFile { root, firstPath, frame in
            let first = try #require(HvfFramebufferMapping(path: firstPath.path))
            let requester = HvfFramebufferFrameRequester()
            requester.clock = { 111 }
            requester.post(first, path: firstPath.path)
            try await waitForRequest(111, at: firstPath)
            let secondPath = root.appendingPathComponent("second.fb")
            try frame.write(to: secondPath)
            let second = try #require(HvfFramebufferMapping(path: secondPath.path))
            requester.clock = { 222 }
            requester.post(second, path: secondPath.path)
            try await waitForRequest(222, at: secondPath)
            #expect(try request(firstPath) == 111, "a reused descriptor wrote the old framebuffer")
            #expect(try request(secondPath) == 222)
        }
    }

    private func withFrameFile(_ body: (URL, URL, Data) async throws -> Void) async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        var frame = Data(count: 68)
        for (offset, value) in [(0, UInt32(0x42564642)), (8, 1), (12, 1), (16, 4), (24, 2), (40, 0x51455242)] {
            var word = value.littleEndian
            withUnsafeBytes(of: &word) { frame.replaceSubrange(offset..<(offset + 4), with: $0) }
        }
        let path = root.appendingPathComponent("display.fb")
        try frame.write(to: path)
        try await body(root, path, frame)
    }

    private func waitForRequest(_ expected: UInt64, at path: URL) async throws {
        let deadline = ContinuousClock.now + .seconds(2)
        while try request(path) != expected, ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(5))
        }
        #expect(try request(path) == expected)
    }

    private func request(_ path: URL) throws -> UInt64 {
        try Data(contentsOf: path)[32..<40].reversed().reduce(0) { $0 << 8 | UInt64($1) }
    }
}
#endif
