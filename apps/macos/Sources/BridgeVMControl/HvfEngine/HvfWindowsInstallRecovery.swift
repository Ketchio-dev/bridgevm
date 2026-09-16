import CryptoKit
import Darwin
import Foundation

/// An existing transaction is resumed without recreating any installation inputs.
enum HvfWindowsInstallRecovery {
    enum Decision: Equatable, Sendable {
        case fresh
        case pending(Ticket)
    }

    struct Ticket: Equatable, Sendable {
        fileprivate let journalSHA256: String
        fileprivate let transactionID: String
    }

    static func inspect(plan: HvfWindowsInstallPlan) throws -> Decision {
        let paths = paths(for: plan)
        for directory in [paths.bundle, paths.metadata] {
            _ = try existing(directory, type: S_IFDIR)
        }
        guard try existing(paths.transaction, type: S_IFDIR) else {
            try validateFreshConfiguration(paths)
            return .fresh
        }
        let (data, journal) = try readPreserved(plan: plan, paths: paths)
        return .pending(Ticket(journalSHA256: digest(data), transactionID: journal.transactionID))
    }

    static func recover(plan: HvfWindowsInstallPlan, ticket: Ticket,
                        secureBootSeeder: HvfWindowsInstallFinalization.SecureBootSeeder =
                            HvfWindowsInstallFinalization.defaultSecureBootSeeder) throws -> VMConfig {
        let paths = paths(for: plan)
        return try withExistingLock(paths) {
            let (data, journal) = try readPreserved(plan: plan, paths: paths)
            guard digest(data) == ticket.journalSHA256,
                  journal.transactionID == ticket.transactionID else {
                throw HvfWindowsInstallFinalizationError.invalidState("복구를 기다리는 동안 설치 기록이 변경되었습니다.")
            }
            try HvfWindowsInstallFinalization.resume(journal, paths: paths, faultInjector: { _ in },
                secureBootSeeder: secureBootSeeder,
                installLog: URL(fileURLWithPath: plan.tmpEvidenceDir).appendingPathComponent("run.log"),
                finalLog: URL(fileURLWithPath: plan.bundleInstallLogPath))
            let config = try HvfWindowsInstallFinalization.loadConfig(paths.config)
            try HvfWindowsInstallFinalization.validateConfig(config, pending: false, paths: paths)
            return config
        }
    }

    private static func paths(for plan: HvfWindowsInstallPlan) -> HvfWindowsInstallFinalizationPaths {
        HvfWindowsInstallFinalization.paths(slug: plan.slug, libraryRoot: plan.libraryRoot,
                                           bundlePath: plan.bundlePath)
    }

    private static func validateFreshConfiguration(_ paths: HvfWindowsInstallFinalizationPaths) throws {
        for directory in [paths.libraryRoot, paths.config.deletingLastPathComponent()] {
            _ = try existing(directory, type: S_IFDIR)
        }
        guard try existing(paths.config, type: S_IFREG) else { return }
        let config = try HvfWindowsInstallFinalization.loadConfig(paths.config)
        try HvfWindowsInstallFinalization.validateConfig(config, pending: true, paths: paths)
    }

    private static func readPreserved(plan: HvfWindowsInstallPlan,
                                      paths: HvfWindowsInstallFinalizationPaths) throws
        -> (Data, HvfWindowsInstallFinalizationJournal) {
        for directory in [paths.libraryRoot, paths.bundle, paths.metadata, paths.transaction] {
            guard try existing(directory, type: S_IFDIR) else {
                throw HvfWindowsInstallFinalizationError.missingArtifact(directory.path)
            }
        }
        let data = try HvfWindowsInstallDurability.readRegularFile(paths.journal,
                                                                 maximumBytes: VMLibrary.maximumConfigBytes)
        let journal = try JSONDecoder().decode(HvfWindowsInstallFinalizationJournal.self, from: data)
        try HvfWindowsInstallFinalization.validate(journal: journal, paths: paths)
        let config = try HvfWindowsInstallFinalization.loadConfig(paths.config)
        // A crash may publish config before advancing the journal; either explicit state can resume.
        try HvfWindowsInstallFinalization.validateConfig(config, pending: config.installPending ?? true, paths: paths)
        let requestPath = journal.phase < .requestStaged ? paths.pendingRequest : paths.stagedRequest
        let preserved = try HvfWindowsInstallRequestSnapshot.load(requestPath, expectedSHA256: journal.requestSHA256)
        guard preserved.request == plan.request else {
            throw HvfWindowsInstallFinalizationError.invalidState("복구할 설치 요청이 승인된 설치 계획과 다릅니다.")
        }
        return (data, journal)
    }

    /// Never use the fresh-install lock constructor: it creates vanished directories and files.
    private static func withExistingLock<T>(_ paths: HvfWindowsInstallFinalizationPaths,
                                            body: () throws -> T) throws -> T {
        let directory = open(paths.transaction.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard directory >= 0 else { throw posixError() }
        defer { close(directory) }
        let lock = openat(directory, "lock", O_RDWR | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        guard lock >= 0 else { throw posixError() }
        defer { close(lock) }
        try requireIdentity(paths.lock, descriptor: lock, type: S_IFREG)
        guard flock(lock, LOCK_EX | LOCK_NB) == 0 else {
            if errno == EWOULDBLOCK || errno == EAGAIN { throw HvfWindowsInstallFinalizationError.transactionBusy }
            throw posixError()
        }
        defer { flock(lock, LOCK_UN) }
        try requireIdentity(paths.transaction, descriptor: directory, type: S_IFDIR)
        try requireIdentity(paths.lock, descriptor: lock, type: S_IFREG)
        return try body()
    }

    private static func requireIdentity(_ url: URL, descriptor: Int32, type: mode_t) throws {
        var held = stat()
        guard fstat(descriptor, &held) == 0 else { throw posixError() }
        guard let current = try status(url), held.st_mode & S_IFMT == type,
              current.st_mode & S_IFMT == type, held.st_dev == current.st_dev,
              held.st_ino == current.st_ino else {
            throw HvfWindowsInstallFinalizationError.unsafePath(url.path)
        }
    }

    private static func existing(_ url: URL, type: mode_t) throws -> Bool {
        guard let value = try status(url) else { return false }
        guard value.st_mode & S_IFMT == type else {
            throw HvfWindowsInstallFinalizationError.unsafePath(url.path)
        }
        return true
    }

    private static func status(_ url: URL) throws -> stat? {
        var value = stat()
        if lstat(url.path, &value) == 0 { return value }
        if errno == ENOENT { return nil }
        throw posixError()
    }

    private static func posixError() -> NSError { NSError(domain: NSPOSIXErrorDomain, code: Int(errno)) }
    private static func digest(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
}
