import Foundation

/// A library VM's Mac folder and the guest path it appears at.
struct HvfSharedFolderSelection: Equatable {
    var hostPath: String
    var guestPath: String
}

extension VMLibrary {
    /// Destination reservation protects IDs, not folder names. Hold from the share scan through
    /// publication so concurrent creation workers in this app cannot choose the same folder.
    static let windowsShareRegistrationLock = NSRecursiveLock()
}

extension VMConfig {
    /// New registrations only: allocating a name does not touch the user's home directory.
    mutating func enableNewWindowsShare(otherRegistrations: [VMConfig],
        home: URL = FileManager.default.homeDirectoryForCurrentUser) {
        let cleaned = name.replacingOccurrences(of: "/", with: "-").replacingOccurrences(of: ":", with: "-")
        let safeName = cleaned == "." || cleaned == ".." ? slug : cleaned
        let parent = home.appendingPathComponent("BridgeVM Shared Folders", isDirectory: true)
        let occupied = Set(otherRegistrations.compactMap(\.sharedFolderHostPath).map(Self.sharePathKey))
        var component = safeName
        var suffix = 2
        while occupied.contains(Self.sharePathKey(parent.appendingPathComponent(component, isDirectory: true).path)) {
            component = "\(safeName) \(suffix)"
            suffix += 1
        }
        // Registrations, not disk contents, own names: deleting a VM frees its folder for reuse.
        sharedFolderHostPath = parent.appendingPathComponent(component, isDirectory: true).path
        sharedFolderGuestPath = "C:\\bridgevm-share"
        createSharedFolderOnStart = true
    }

    private static func sharePathKey(_ path: String) -> String {
        URL(fileURLWithPath: path).standardizedFileURL.path
            .precomposedStringWithCanonicalMapping
            .folding(options: [.caseInsensitive], locale: Locale(identifier: "en_US_POSIX"))
    }

    /// nil unless vm.json holds both halves. An incomplete hand-edited pair still reaches the
    /// launch config, where readiness refuses it instead of sharing anything else.
    var sharedFolder: HvfSharedFolderSelection? {
        guard let host = sharedFolderHostPath, let guest = sharedFolderGuestPath else { return nil }
        return HvfSharedFolderSelection(hostPath: host, guestPath: guest)
    }
}

extension HvfEngineConfig {
    static let shareUnavailableMessage = "Folder sharing is off for this start because the shared folder is unavailable. Choose the folder again in Settings."

    /// A launch-only copy: never clears or rewrites the saved preference.
    func preparingSharedFolder(diagnostic: (String) -> Void) -> HvfEngineConfig {
        var launch = self
        guard let host = shareHostDir, host.hasPrefix("/"), shareGuestDir?.isEmpty == false else { return launch }
        do {
            if libraryContext?.config.createSharedFolderOnStart == true {
                try FileManager.default.createDirectory(atPath: host, withIntermediateDirectories: true, attributes: nil)
            }
            var directory: ObjCBool = false
            if FileManager.default.fileExists(atPath: host, isDirectory: &directory), !directory.boolValue {
                throw CocoaError(.fileWriteFileExists)
            }
        } catch {
            launch.shareHostDir = nil
            launch.shareGuestDir = nil
            diagnostic(Self.shareUnavailableMessage)
        }
        return launch
    }

    /// The shared folder's name while its Mac folder is missing or unreachable (an unplugged drive, a
    /// deleted folder). Such a VM still starts with that same folder: the runtime shares it once it
    /// appears (HvfShareRootNotice tells the user).
    func missingShareFolder(fileManager: FileManager = .default) -> String? {
        guard let host = shareHostDir, host.hasPrefix("/"), shareGuestDir?.isEmpty == false,
              !fileManager.fileExists(atPath: host) else { return nil }
        return (host as NSString).lastPathComponent
    }
}

/// New VMs have a dedicated default; existing registrations are never migrated. A saved folder
/// that is missing at launch is still the one shared once it appears; no other folder is substituted for it.
extension HvfEngineSession {
    /// vm.json's pair for a library VM after the attempt (the selection itself otherwise), plus any message.
    func applySharedFolderSelection(_ selection: HvfSharedFolderSelection?,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) })
        -> (selection: HvfSharedFolderSelection?, message: String?) {
        guard persistsLibraryPreferences else {
            config.shareHostDir = selection?.hostPath
            config.shareGuestDir = selection?.guestPath
            return (selection, nil)
        }
        let message = saveSharedFolderPreference(selection, publish: publish)
        return (config.libraryContext?.config.sharedFolder, message)
    }

    /// nil stops sharing. Like launch, a selection may name a Mac folder that is missing for now (a new
    /// VM's default before its first start, an unplugged drive); a path to a file is refused.
    func saveSharedFolderPreference(_ selection: HvfSharedFolderSelection?,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> String? {
        guard persistsLibraryPreferences else { return nil }
        if let selection {
            var isDirectory: ObjCBool = false
            let exists = FileManager.default.fileExists(atPath: selection.hostPath, isDirectory: &isDirectory)
            guard selection.hostPath.hasPrefix("/"), !exists || isDirectory.boolValue,
                  !selection.guestPath.trimmingCharacters(in: .whitespaces).isEmpty else {
                return "Enter the full path to a Mac folder and a destination path in Windows."
            }
        }
        return saveLibraryPreference("Shared folder", publish: publish) {
            // A new VM's default folder is still created at start; a folder the user chose never is.
            if $0.sharedFolderHostPath != selection?.hostPath { $0.createSharedFolderOnStart = nil }
            $0.sharedFolderHostPath = selection?.hostPath
            $0.sharedFolderGuestPath = selection?.guestPath
        }
    }

    /// Runtime-view Start launches only what vm.json holds, so a typed but unsaved pair is
    /// saved first. Returns nil when the start may proceed, otherwise why it may not.
    func saveSharedFolderBeforeStart(_ visible: HvfSharedFolderSelection?) -> String? {
        guard persistsLibraryPreferences, let saved = config.libraryContext?.config,
              visible?.hostPath != saved.sharedFolderHostPath
                || visible?.guestPath != saved.sharedFolderGuestPath else { return nil }
        let result = applySharedFolderSelection(visible)
        guard result.selection == visible else {
            return result.message ?? "Couldn’t save the shared folder. Check the folder settings and try starting again."
        }
        return result.message
    }
}
