import Foundation

/// Clipboard sync is two-way, so a library VM's choice is saved in vm.json and every
/// start path (runtime view, dashboard, native CLI) launches with it. The experimental
/// non-library entry keeps the view-local toggle.
extension HvfEngineSession {
    typealias RegistrationPublish = (Data, URL) -> VMRegistrationCommitOutcome
    var persistsClipboardSyncPreference: Bool { config.libraryContext != nil }

    /// A running or busy VM keeps the launch it was admitted with; saving then would
    /// also orphan its ownership, so the preference changes only while fully stopped.
    var clipboardSyncPreferenceEditable: Bool {
        !hasActiveRuntimeWork && !hasRetainedAttachment && !workAdmissionGate.isChecking
    }

    /// What the toggle shows afterwards: vm.json's value for a library VM, plus any message.
    func applyClipboardSyncToggle(_ enabled: Bool,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (isOn: Bool, message: String?) {
        guard persistsClipboardSyncPreference else { return (enabled, nil) }
        let message = saveClipboardSyncPreference(enabled, publish: publish)
        return (config.libraryContext?.config.clipboardSyncEnabled ?? true, message)
    }

    /// Returns nil once vm.json holds `enabled`, otherwise the refusal or warning.
    /// `true` is saved as an absent key so re-enabled VMs match never-touched ones.
    func saveClipboardSyncPreference(_ enabled: Bool,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> String? {
        guard let context = config.libraryContext else { return nil }
        let stopFirst = "VM을 완전히 중지한 뒤 클립보드 동기화 설정을 변경하세요."
        guard clipboardSyncPreferenceEditable else { return stopFirst }
        var updated = context.config
        updated.id = context.config.slug
        updated.clipboardSyncEnabled = enabled ? nil : false
        guard updated.clipboardSyncEnabled != context.config.clipboardSyncEnabled else { return nil }
        guard let persisted = onConfigurationPersisted, workAdmission != nil else {
            return "VM 라이브러리를 사용할 수 없습니다. 라이브러리 화면에서 다시 시도하세요."
        }
        if let refusal = workAdmissionGate.check(workAdmission, reportRefusal: false) { return refusal }
        guard clipboardSyncPreferenceEditable else { return stopFirst }
        // As for clone and move, a VM process this app is not attached to still owns the disk.
        guard !diskProcessIsRunning(context) else {
            return "이 VM의 디스크를 사용하는 프로세스가 실행 중입니다. VM을 완전히 중지한 뒤 다시 시도하세요."
        }
        guard VMLibrary.isCurrentRegistration(context.config, rootURL: context.rootURL) else {
            return "VM 등록 정보가 변경되었습니다. 목록을 새로고침한 뒤 다시 시도하세요."
        }
        switch VMLibrary.saveOutcome(updated, rootURL: context.rootURL, publish: publish) {
        case .notPublished:
            return "클립보드 동기화 설정을 저장하지 못했습니다. 저장된 설정은 바뀌지 않았습니다."
        case .committed:
            persisted(context.config, updated)
            return nil
        case .publishedButUnsynced:
            // vm.json already holds the new value, so the app must show it too.
            persisted(context.config, updated)
            return "클립보드 동기화 설정을 vm.json에 기록했지만 디스크 동기화를 확인하지 못했습니다. 설정을 다시 확인하세요."
        }
    }

    private func diskProcessIsRunning(_ context: HvfLibraryLaunchContext) -> Bool {
        var disks = [config.targetDiskPath]
        if let saved = HvfEngineConfig.libraryVM(context.config, rootURL: context.rootURL)?.targetDiskPath,
           saved != config.targetDiskPath { disks.append(saved) }
        return disks.contains { !$0.isEmpty && processIsRunning($0) }
    }
}
