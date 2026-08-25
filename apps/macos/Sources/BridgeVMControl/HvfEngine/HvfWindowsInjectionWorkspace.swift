import Darwin
import Foundation

enum HvfWindowsInjectionWorkspace {
    enum WorkspaceError: Error { case unsafePath, copyFailed, evidenceMissing }

    static func reset(_ plan: HvfWindowsInstallPlan) throws {
        let root = URL(fileURLWithPath: plan.injectionRootPath, isDirectory: true)
        guard HvfPrivateSnapshotPath.isPrivateParent(root) else { throw WorkspaceError.unsafePath }
        let work = URL(fileURLWithPath: plan.injectionWorkPath, isDirectory: true)
        if HvfPrivateSnapshotPath.entryExists(work) {
            guard ownedDirectory(work) else { throw WorkspaceError.unsafePath }
            try FileManager.default.removeItem(at: work)
        }
        try FileManager.default.createDirectory(
            at: work, withIntermediateDirectories: false,
            attributes: [.posixPermissions: 0o700])
    }

    static func cloneImportedMedia(_ plan: HvfWindowsInstallPlan) throws {
        for path in [plan.tmpTargetPath, plan.tmpVarsPath] { try removeOwnedRegularFile(path) }
        guard VMLibrary.cloneOrCopyFile(from: plan.bundleDiskPath, to: plan.tmpTargetPath),
              VMLibrary.cloneOrCopyFile(from: plan.bundleVarsPath, to: plan.tmpVarsPath) else {
            try? removeOwnedRegularFile(plan.tmpTargetPath)
            try? removeOwnedRegularFile(plan.tmpVarsPath)
            throw WorkspaceError.copyFailed
        }
    }

    static func retainEvidence(_ plan: HvfWindowsInstallPlan) throws {
        let source = URL(fileURLWithPath: plan.injectionEvidencePath, isDirectory: true)
        let hashSource = URL(fileURLWithPath: plan.injectorImagePath + ".sha256")
        guard ownedDirectory(source), ownedRegularFile(hashSource),
              let hash = try? String(contentsOf: hashSource, encoding: .ascii)
                .trimmingCharacters(in: .whitespacesAndNewlines),
              hash.count == 64, hash.allSatisfy({ $0.isHexDigit && !$0.isUppercase }),
              plan.injectionBootWasObserved() else {
            throw WorkspaceError.evidenceMissing
        }
        let destination = URL(
            fileURLWithPath: plan.retainedInjectionEvidencePath, isDirectory: true)
        if HvfPrivateSnapshotPath.entryExists(destination) {
            guard ownedDirectory(destination) else { throw WorkspaceError.unsafePath }
            try FileManager.default.removeItem(at: destination)
        }
        try FileManager.default.copyItem(at: source, to: destination)
        try Data("\(hash)\n".utf8).write(
            to: destination.appendingPathComponent("injector.sha256"), options: [.atomic])
    }

    static func cleanup(_ plan: HvfWindowsInstallPlan) {
        let work = URL(fileURLWithPath: plan.injectionWorkPath, isDirectory: true)
        if ownedDirectory(work) { try? FileManager.default.removeItem(at: work) }
        for path in [plan.tmpTargetPath, plan.tmpVarsPath] { try? removeOwnedRegularFile(path) }
    }

    private static func removeOwnedRegularFile(_ path: String) throws {
        var status = stat()
        guard lstat(path, &status) == 0 else {
            if errno == ENOENT { return }
            throw WorkspaceError.unsafePath
        }
        guard status.st_uid == geteuid(), status.st_mode & S_IFMT == S_IFREG else {
            throw WorkspaceError.unsafePath
        }
        try FileManager.default.removeItem(atPath: path)
    }

    private static func ownedDirectory(_ url: URL) -> Bool {
        var status = stat()
        return lstat(url.path, &status) == 0 && status.st_uid == geteuid()
            && status.st_mode & S_IFMT == S_IFDIR
    }

    private static func ownedRegularFile(_ url: URL) -> Bool {
        var status = stat()
        return lstat(url.path, &status) == 0 && status.st_uid == geteuid()
            && status.st_mode & S_IFMT == S_IFREG
    }
}
