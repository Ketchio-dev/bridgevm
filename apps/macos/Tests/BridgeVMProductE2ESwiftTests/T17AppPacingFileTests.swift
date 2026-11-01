// sources: apps/macos/Sources/BridgeVMProductE2E/T17AppPacingFile.swift
import Darwin
import Foundation
import Testing

@Suite struct T17AppPacingFileTests {
    private func withDirectory(_ body: (URL) throws -> Void) throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        try body(directory)
    }

    @Test func readsOnlyRegularFilesWithinTheBound() throws {
        try withDirectory { directory in
            let file = directory.appendingPathComponent("app-pacing.log")
            let data = Data("pacing\n".utf8)
            try data.write(to: file)
            #expect(T17AppPacingFile.read(file, limit: data.count) == data)
            #expect(T17AppPacingFile.read(file, limit: data.count - 1) == nil)
            #expect(T17AppPacingFile.read(file, limit: -1) == nil)
            #expect(T17AppPacingFile.read(directory, limit: 64 * 1024) == nil)
            #expect(T17AppPacingFile.read(directory.appendingPathComponent("missing"), limit: 64 * 1024) == nil)
        }
    }

    @Test func refusesSymlinksAndFifosWithoutWaitingForAWriter() throws {
        try withDirectory { directory in
            let outside = directory.appendingPathComponent("outside.log")
            try Data("private\n".utf8).write(to: outside)
            let link = directory.appendingPathComponent("link.log"), fifo = directory.appendingPathComponent("fifo")
            try FileManager.default.createSymbolicLink(at: link, withDestinationURL: outside)
            #expect(T17AppPacingFile.read(link, limit: 64 * 1024) == nil)
            #expect(Darwin.mkfifo(fifo.path, 0o600) == 0)
            #expect(T17AppPacingFile.read(fifo, limit: 64 * 1024) == nil)
        }
    }

    @Test func readsTheOpenedFileWhenItsPathIsReplacedByALink() throws {
        try withDirectory { directory in
            let file = directory.appendingPathComponent("app-pacing.log"), original = Data("original\n".utf8)
            try original.write(to: file)
            let descriptor = Darwin.open(file.path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
            #expect(descriptor >= 0)
            guard descriptor >= 0 else { return }
            defer { Darwin.close(descriptor) }
            let outside = directory.appendingPathComponent("outside.log")
            try Data("different private data\n".utf8).write(to: outside)
            try FileManager.default.removeItem(at: file)
            try FileManager.default.createSymbolicLink(at: file, withDestinationURL: outside)
            #expect(T17AppPacingFile.read(descriptor: descriptor, limit: 64 * 1024) == original)
            #expect(T17AppPacingFile.read(file, limit: 64 * 1024) == nil)
        }
    }
}
