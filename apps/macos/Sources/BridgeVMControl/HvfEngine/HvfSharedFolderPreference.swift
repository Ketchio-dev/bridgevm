import Foundation

/// A host folder the user explicitly chose for a library VM and the guest path it appears at.
struct HvfSharedFolderSelection: Equatable {
    var hostPath: String
    var guestPath: String
}

extension VMConfig {
    /// nil unless vm.json holds both halves. An incomplete hand-edited pair still reaches the
    /// launch config, where readiness refuses it instead of sharing anything else.
    var sharedFolder: HvfSharedFolderSelection? {
        guard let host = sharedFolderHostPath, let guest = sharedFolderGuestPath else { return nil }
        return HvfSharedFolderSelection(hostPath: host, guestPath: guest)
    }
}

extension HvfEngineConfig {
    /// The shared folder's name while its Mac folder is missing or unreachable (an unplugged drive, a
    /// deleted folder). Such a VM still starts with that same folder: the runtime shares it once it
    /// appears (HvfShareRootNotice tells the user).
    func missingShareFolder(fileManager: FileManager = .default) -> String? {
        guard let host = shareHostDir, host.hasPrefix("/"), shareGuestDir?.isEmpty == false,
              !fileManager.fileExists(atPath: host) else { return nil }
        return (host as NSString).lastPathComponent
    }
}

/// Sharing is never enabled by default and only a user's selection is saved. A saved folder
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

    /// nil stops sharing. A selection is saved only while its host folder exists as a directory.
    func saveSharedFolderPreference(_ selection: HvfSharedFolderSelection?,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> String? {
        guard persistsLibraryPreferences else { return nil }
        if let selection {
            var isDirectory: ObjCBool = false
            guard selection.hostPath.hasPrefix("/"),
                  FileManager.default.fileExists(atPath: selection.hostPath, isDirectory: &isDirectory),
                  isDirectory.boolValue,
                  !selection.guestPath.trimmingCharacters(in: .whitespaces).isEmpty else {
                return "공유 폴더를 저장하려면 존재하는 호스트 디렉터리의 절대 경로와 게스트 경로가 모두 필요합니다."
            }
        }
        return saveLibraryPreference("공유 폴더 설정", publish: publish) {
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
            return result.message ?? "공유 폴더 설정을 저장하지 못했습니다. 설정을 확인한 뒤 다시 시작하세요."
        }
        return result.message
    }
}
