import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfOwnedRuntimeChannelTests: XCTestCase {
    private func peer(_ handle: FileHandle) throws -> FileHandle {
        let fd = dup(handle.fileDescriptor)
        guard fd >= 0 else { throw CocoaError(.fileReadUnknown) }
        return FileHandle(fileDescriptor: fd, closeOnDealloc: true)
    }
    private func hello(_ input: FileHandle) throws {
        var descriptor = pollfd(fd: input.fileDescriptor, events: Int16(POLLIN), revents: 0)
        XCTAssertEqual(Darwin.poll(&descriptor, 1, 2000), 1)
        guard descriptor.revents & Int16(POLLIN) != 0 else { throw CocoaError(.fileReadUnknown) }
        var storage = [UInt8](repeating: 0, count: 8196)
        let count = Darwin.read(input.fileDescriptor, &storage, storage.count)
        guard count > 0 else { throw CocoaError(.fileReadUnknown) }
        var bytes = Data(storage.prefix(count))
        let body = try XCTUnwrap(HvfOwnedRuntimeCodec.takeFrame(from: &bytes))
        _ = try HvfOwnedRuntimeCodec.decode(HvfOwnedRuntimeHello.self, payload: body)
        XCTAssertTrue(bytes.isEmpty)
    }
    private func start(_ channel: HvfOwnedRuntimeChannel) throws {
        let hello = try HvfOwnedRuntimeCodec.decode(HvfOwnedRuntimeHello.self,
            payload: HvfOwnedRuntimeCodecTests.data("hello-no-key"))
        channel.start(helloFrame: try HvfOwnedRuntimeCodec.frame(hello))
    }

    func testStopEPIPEStillDrainsBufferedSpontaneousCompleteAndEOF() async throws {
        let channel = try HvfOwnedRuntimeChannel()
        let input = try peer(channel.input.fileHandleForReading)
        let output = try peer(channel.output.fileHandleForWriting)
        defer { try? input.close(); try? output.close(); channel.close() }
        let finished = HvfOwnedRuntimeChannelTestWait("terminal transcript drained")
        var messages = [HvfOwnedRuntimeChannelEvent]()
        let reader = Task {
            for await event in channel.events { messages.append(event) }
            finished.complete()
        }
        try start(channel); try hello(input)
        try input.close() // Only the peer read end: subsequent STOP gets an actual EPIPE.
        let stop = try HvfOwnedRuntimeCodec.decode(HvfOwnedRuntimeStop.self, payload: HvfOwnedRuntimeCodecTests.data("stop"))
        try channel.sendStop(stop)
        let complete = try HvfOwnedRuntimeCodecTests.event("complete-not-admitted")
        try output.write(contentsOf: HvfOwnedRuntimeCodec.frame(complete))
        try output.close()
        try await finished.wait()
        channel.close(); await reader.value
        XCTAssertEqual(messages.count, 2)
        guard messages.count == 2 else { return }
        if case let .message(actual) = messages[0] { XCTAssertEqual(actual, complete) }
        else { XCTFail("Valid buffered completion must survive an independently closed input pipe") }
        if case .eof = messages[1] {} else { XCTFail("Terminal EOF must be read, never synthesized") }
    }

    func testPartialFrameEOFIsAProtocolFailure() async throws {
        let channel = try HvfOwnedRuntimeChannel()
        let input = try peer(channel.input.fileHandleForReading)
        let output = try peer(channel.output.fileHandleForWriting)
        defer { try? input.close(); try? output.close(); channel.close() }
        let finished = HvfOwnedRuntimeChannelTestWait("partial EOF rejected")
        var failure: HvfOwnedRuntimeProtocolError?
        let reader = Task {
            for await event in channel.events { if case let .failed(error) = event { failure = error } }
            finished.complete()
        }
        try start(channel); try hello(input)
        try output.write(contentsOf: Data([0, 0, 0, 20, 123])); try output.close()
        try await finished.wait()
        channel.close(); await reader.value
        XCTAssertEqual(failure, .invalidFrame)
    }

    func testIncompleteFrameUsesItsOriginalTwoSecondDeadline() async throws {
        let channel = try HvfOwnedRuntimeChannel()
        let input = try peer(channel.input.fileHandleForReading)
        let output = try peer(channel.output.fileHandleForWriting)
        defer { try? input.close(); try? output.close(); channel.close() }
        let finished = HvfOwnedRuntimeChannelTestWait("partial frame deadline")
        var failure: HvfOwnedRuntimeProtocolError?
        let reader = Task {
            for await event in channel.events { if case let .failed(error) = event { failure = error } }
            finished.complete()
        }
        try start(channel); try hello(input)
        try output.write(contentsOf: Data([0]))
        try await finished.wait()
        channel.close(); await reader.value
        XCTAssertEqual(failure, .timedOut)
    }
}
