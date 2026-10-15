import XCTest
@testable import BridgeVMControl

final class HvfPendingDeletionFixture {
    let root: URL
    let library: URL
    let iso: URL
    let config: VMConfig
    let fm = FileManager.default
    var bundle: URL { URL(fileURLWithPath: config.bundlePath) }
    var entry: URL { library.appendingPathComponent(config.slug) }
    var command: URL { root.appendingPathComponent("command") }

    init(external: Bool = false) throws {
        root = LibraryDeletionTestDirectory.temporary.resolvingSymlinksInPath()
            .appendingPathComponent("pending-delete-" + UUID().uuidString)
        library = root.appendingPathComponent("library")
        iso = root.appendingPathComponent("source.iso")
        try fm.createDirectory(at: root, withIntermediateDirectories: true)
        try Data("synthetic installer bytes".utf8).write(to: iso)
        _ = try XCTUnwrap(VMLibrary.createWindowsHVFInstall(name: "Pending", isoPath: iso.path,
            diskGiB: 64, injectViogpu3d: false, driverPackageDir: nil,
            storageDir: external ? root.appendingPathComponent("external") : nil,
            libraryRoot: library))
        config = try NativeLibraryReader.readConfig(rootURL: library, id: "pending")
    }

    deinit { try? fm.removeItem(at: root) }

    func save(_ config: VMConfig) throws {
        try JSONEncoder().encode(config).write(to: entry.appendingPathComponent("vm.json"), options: .atomic)
    }

    func file(_ relative: String) throws {
        let url = bundle.appendingPathComponent(relative)
        try fm.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("residue".utf8).write(to: url)
    }

    func directory(_ relative: String) throws {
        try fm.createDirectory(at: bundle.appendingPathComponent(relative), withIntermediateDirectories: true)
    }

    func danglingLink(_ relative: String) throws {
        try fm.createSymbolicLink(at: bundle.appendingPathComponent(relative),
                                  withDestinationURL: root.appendingPathComponent("missing"))
    }

    func delete(_ selected: VMConfig? = nil, beforeReady: String = "", refuse: Bool = false) throws -> Bool {
        let repository = root.appendingPathComponent("helper-repository")
        let helper = repository.appendingPathComponent("target/release/examples/snapshot_pair_cli")
        let wrapper = repository.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh")
        for url in [helper, wrapper] {
            try fm.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        }
        let body = """
        #!/bin/sh
        printf '%s\\n' "$@" > '\(command.path)'
        \(beforeReady)
        \(refuse ? "exit 1" : "printf 'bridgevm-media-lease-v1\\n'; read command; test \"$command\" = release")
        """
        try Data(body.utf8).write(to: helper)
        try Data("#!/bin/sh\nexit 99\n".utf8).write(to: wrapper)
        for url in [helper, wrapper] { try fm.setAttributes([.posixPermissions: 0o700], ofItemAtPath: url.path) }
        let prior = ProcessInfo.processInfo.environment["BRIDGEVM_REPO_ROOT"]
        setenv("BRIDGEVM_REPO_ROOT", repository.path, 1)
        defer {
            if let prior { setenv("BRIDGEVM_REPO_ROOT", prior, 1) } else { unsetenv("BRIDGEVM_REPO_ROOT") }
        }
        return LibraryDeletionProtection.delete(selected ?? config, rootURL: library)
    }

    func assertRefused(file: StaticString = #filePath, line: UInt = #line) throws {
        XCTAssertThrowsError(try HvfPendingDeletionGuard.validate(config, rootURL: library), file: file, line: line)
        XCTAssertFalse(try delete(), file: file, line: line)
        XCTAssertTrue(fm.fileExists(atPath: entry.path), file: file, line: line)
        XCTAssertFalse(fm.fileExists(atPath: command.path), file: file, line: line)
    }
}
