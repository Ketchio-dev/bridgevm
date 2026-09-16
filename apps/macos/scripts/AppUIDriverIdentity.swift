import AppKit
import CryptoKit
import Darwin
import Security

@MainActor
enum AppUIDriverIdentity {
    @discardableResult
    static func verify(_ identity: AppUIDriverProcessIdentity, privateRoot: URL, driver: Bool) throws -> NSRunningApplication {
        let bundle = privateRoot.appendingPathComponent(driver ? "BridgeVMAppUIDriver.app" : "BridgeVMAppUIHost.app")
        let executable = bundle.appendingPathComponent("Contents/MacOS/" + (driver ? "AppUIHostLauncher" : "BridgeVMControl"))
        let identifier = driver ? AppUIDriverConstants.driverBundleIdentifier : AppUIDriverConstants.hostBundleIdentifier
        guard identity.pid > 1, identity.bundlePath == bundle.path, identity.executablePath == executable.path,
              identity.bundleIdentifier == identifier,
              bundle.resolvingSymlinksInPath().path == bundle.path,
              executable.resolvingSymlinksInPath().path == executable.path,
              let running = NSRunningApplication(processIdentifier: identity.pid), !running.isTerminated,
              running.bundleIdentifier == identifier, running.bundleURL?.resolvingSymlinksInPath().path == bundle.path,
              running.executableURL?.resolvingSymlinksInPath().path == executable.path,
              running.launchDate?.timeIntervalSince1970 == identity.launchDate,
              try digest(executable) == identity.executableSHA256 else { throw AppUIDriverFailure.identityMismatch }
        if driver {
            guard identity.pid == getpid(), Bundle.main.bundleURL.resolvingSymlinksInPath().path == bundle.path,
                  Bundle.main.bundleIdentifier == identifier,
                  Bundle.main.executableURL?.resolvingSymlinksInPath().path == executable.path else {
                throw AppUIDriverFailure.identityMismatch
            }
        }
        return running
    }

    static func digest(_ path: URL) throws -> String {
        let fd = open(path.path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        guard fd >= 0 else { throw AppUIDriverFailure.invalidFile }
        defer { close(fd) }
        var before = stat(), after = stat()
        guard fstat(fd, &before) == 0, before.st_mode & S_IFMT == S_IFREG,
              before.st_size > 0, before.st_size <= 536_870_912 else { throw AppUIDriverFailure.invalidFile }
        var hash = SHA256(), bytes = [UInt8](repeating: 0, count: 1_048_576), total = 0
        while true {
            let count = read(fd, &bytes, bytes.count)
            if count < 0 && errno == EINTR { continue }
            guard count >= 0 else { throw AppUIDriverFailure.ioFailure }
            if count == 0 { break }
            total += count
            guard total <= before.st_size else { throw AppUIDriverFailure.invalidFile }
            hash.update(data: Data(bytes.prefix(count)))
        }
        guard fstat(fd, &after) == 0, total == before.st_size, before.st_dev == after.st_dev,
              before.st_ino == after.st_ino, before.st_size == after.st_size,
              before.st_mtimespec.tv_sec == after.st_mtimespec.tv_sec,
              before.st_mtimespec.tv_nsec == after.st_mtimespec.tv_nsec,
              before.st_ctimespec.tv_sec == after.st_ctimespec.tv_sec,
              before.st_ctimespec.tv_nsec == after.st_ctimespec.tv_nsec else { throw AppUIDriverFailure.invalidFile }
        return hash.finalize().map { String(format: "%02x", $0) }.joined()
    }

    static func codeRequirement() -> (requirement: String?, unavailable: String?) {
        var code: SecCode?
        guard SecCodeCopySelf([], &code) == errSecSuccess, let code else { return (nil, "copy-self-failed") }
        var staticCode: SecStaticCode?
        guard SecCodeCopyStaticCode(code, [], &staticCode) == errSecSuccess, let staticCode
        else { return (nil, "copy-static-code-failed") }
        var requirement: SecRequirement?
        guard SecCodeCopyDesignatedRequirement(staticCode, [], &requirement) == errSecSuccess, let requirement
        else { return (nil, "copy-designated-requirement-failed") }
        var text: CFString?
        guard SecRequirementCopyString(requirement, [], &text) == errSecSuccess, let text,
              (text as String).utf8.count <= 2_048 else { return (nil, "requirement-string-unavailable") }
        return (text as String, nil)
    }
}
