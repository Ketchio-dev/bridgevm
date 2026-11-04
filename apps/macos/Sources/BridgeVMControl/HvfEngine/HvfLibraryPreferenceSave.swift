import Foundation

/// A library VM's runtime-view launch preferences (clipboard sync, network, durable disk writes,
/// microphone, display resolution, Retina resolution, Command key, shared folder, display driver) are saved in vm.json, so every start path (runtime view,
/// dashboard, native CLI, relaunch) uses them. The experimental non-library entry keeps
/// view-local options.
extension HvfEngineSession {
    typealias RegistrationPublish = (Data, URL) -> VMRegistrationCommitOutcome
    var persistsLibraryPreferences: Bool { config.libraryContext != nil }

    /// A running or busy VM keeps the launch it was admitted with; saving then would
    /// also orphan its ownership, so preferences change only while fully stopped.
    var libraryPreferencesEditable: Bool {
        !hasActiveRuntimeWork && !hasRetainedAttachment && !workAdmissionGate.isChecking
    }

    /// Returns nil once vm.json holds `change` (or already did), otherwise the refusal or warning.
    /// Only the keys `change` sets differ from the saved registration; nothing else is migrated.
    func saveLibraryPreference(_ setting: String, publish: RegistrationPublish,
                               change: (inout VMConfig) -> Void) -> String? {
        guard let context = config.libraryContext else { return nil }
        let stopFirst = "VM을 완전히 중지한 뒤 \(setting)을 변경하세요."
        guard libraryPreferencesEditable else { return stopFirst }
        var current = context.config
        current.id = context.config.slug
        var updated = current
        change(&updated)
        guard updated != current else { return nil }
        guard let persisted = onConfigurationPersisted, workAdmission != nil else {
            return "VM 라이브러리를 사용할 수 없습니다. 라이브러리 화면에서 다시 시도하세요."
        }
        if let refusal = workAdmissionGate.check(workAdmission, reportRefusal: false) { return refusal }
        guard libraryPreferencesEditable else { return stopFirst }
        // As for clone and move, a VM process this app is not attached to still owns the disk.
        guard !diskProcessIsRunning(context) else {
            return "이 VM의 디스크를 사용하는 프로세스가 실행 중입니다. VM을 완전히 중지한 뒤 다시 시도하세요."
        }
        guard VMLibrary.isCurrentRegistration(context.config, rootURL: context.rootURL) else {
            return "VM 등록 정보가 변경되었습니다. 목록을 새로고침한 뒤 다시 시도하세요."
        }
        switch VMLibrary.saveOutcome(updated, rootURL: context.rootURL, publish: publish) {
        case .notPublished:
            return "\(setting)을 저장하지 못했습니다. 저장된 설정은 바뀌지 않았습니다."
        case .committed:
            persisted(context.config, updated)
            return nil
        case .publishedButUnsynced:
            // vm.json already holds the new value, so the app must show it too.
            persisted(context.config, updated)
            return "\(setting)을 vm.json에 기록했지만 디스크 동기화를 확인하지 못했습니다. 설정을 다시 확인하세요."
        }
    }

    private func diskProcessIsRunning(_ context: HvfLibraryLaunchContext) -> Bool {
        var disks = [config.targetDiskPath]
        if let saved = HvfEngineConfig.libraryVM(context.config, rootURL: context.rootURL)?.targetDiskPath,
           saved != config.targetDiskPath { disks.append(saved) }
        return disks.contains { !$0.isEmpty && processIsRunning($0) }
    }
}

extension VMConfig {
    /// This registration with `other`'s runtime-view preference keys: equal to `other`
    /// exactly when only those keys differ.
    func keepingRuntimePreferences(of other: VMConfig) -> VMConfig {
        var copy = self
        copy.clipboardSyncEnabled = other.clipboardSyncEnabled
        copy.networkEnabled = other.networkEnabled
        copy.durableDiskWritesEnabled = other.durableDiskWritesEnabled
        copy.microphoneEnabled = other.microphoneEnabled
        copy.guestResolution = other.guestResolution
        copy.retinaResolution = other.retinaResolution
        copy.commandKeyMode = other.commandKeyMode
        copy.virtioDisplayDriverInstalled = other.virtioDisplayDriverInstalled
        copy.hardwareCursor = other.hardwareCursor
        copy.sharedFolderHostPath = other.sharedFolderHostPath
        copy.sharedFolderGuestPath = other.sharedFolderGuestPath
        return copy
    }
}

extension HvfEngineConfig {
    /// Takes only the preferences the save changed from `launch` (built from `saved`); every
    /// other accepted option, including unsaved view edits, is kept.
    func adoptingSavedPreferences(_ launch: HvfEngineConfig, previous: VMConfig, saved: VMConfig) -> HvfEngineConfig {
        var rebound = self
        rebound.libraryContext = launch.libraryContext
        if saved.clipboardSyncEnabled != previous.clipboardSyncEnabled { rebound.clipboardSync = launch.clipboardSync }
        if saved.networkEnabled != previous.networkEnabled { rebound.virtioNet = launch.virtioNet }
        if saved.durableDiskWritesEnabled != previous.durableDiskWritesEnabled { rebound.durableDiskWrites = launch.durableDiskWrites }
        if saved.microphoneEnabled != previous.microphoneEnabled { rebound.microphoneEnabled = launch.microphoneEnabled }
        if saved.guestResolution != previous.guestResolution { rebound.guestResolution = launch.guestResolution }
        if saved.retinaResolution != previous.retinaResolution { rebound.retinaResolution = launch.retinaResolution }
        if saved.commandKeyMode != previous.commandKeyMode { rebound.commandKeyMode = launch.commandKeyMode }
        if saved.virtioDisplayDriverInstalled != previous.virtioDisplayDriverInstalled { rebound.virtioGpu2d = launch.virtioGpu2d }
        if saved.sharedFolderHostPath != previous.sharedFolderHostPath
            || saved.sharedFolderGuestPath != previous.sharedFolderGuestPath {
            rebound.shareHostDir = launch.shareHostDir
            rebound.shareGuestDir = launch.shareGuestDir
        }
        return rebound
    }
}
