import Foundation

typealias LibraryActionJob = @Sendable () async -> Void
typealias LibraryActionScheduler = @MainActor (@escaping LibraryActionJob) -> Void

enum LibraryFileAction { case deletion, clone, move }

private enum LibraryBusy {
    static let unregistered = "VM 등록 정보가 변경되었거나 제거되었습니다. 목록을 새로고침한 뒤 다시 시도하세요."
    static let fileAction = "이 VM의 삭제·복제·이동 작업이 진행 중입니다. 완료 후 다시 시도하세요."
    static let install = "Windows 설치가 진행 중입니다. 설치를 완료하거나 취소한 뒤 다시 시도하세요."
    static let runtime = "VM이 실행 중이거나 시작·중지 처리 중입니다. 제어 화면에서 완전히 중지한 뒤 다시 시도하세요."
    static let control = "이 VM의 제어 작업이 진행 중입니다. 완료 후 다시 시도하세요."
}

extension LibraryModel {
    /// Why this VM is busy with file, install, runtime or control work, or nil.
    /// Shared by library file actions and runtime/install admission; the exclusions let an
    /// operation's own reservation pass its own check.
    func busyReason(
        slug: String, excluding reservation: (HvfEngineSession, UUID)? = nil,
        excludingInstall: NativeInstallOperation? = nil, excludingWindowsInstall: HvfWindowsInstallSession? = nil
    ) -> String? {
        if deletingSlugs.contains(slug) || cloningSlugs.contains(slug) || movingSlugs.contains(slug) {
            return LibraryBusy.fileAction
        }
        if (windowsInstallSessions.isActive(slug: slug) && windowsInstallSessions.record(for: slug)?.session !== excludingWindowsInstall) || windowsInstallSessions.cliOperations.isActive(vmID: slug, excluding: excludingInstall) || retainedControlStore.records.contains(where: {
            if case let .install(config, session) = $0.descriptor { return config.slug == slug && session.isRunning && session !== excludingWindowsInstall }
            return false
        }) {
            return LibraryBusy.install
        }
        if runtimeWorkConflicts(slug: slug, excluding: reservation) { return LibraryBusy.runtime }
        if hasAcceptedControlOperation(for: slug) { return LibraryBusy.control }
        return nil
    }

    func libraryActionRefusal(for cfg: VMConfig) -> String? {
        guard vms.first(where: { $0.slug == cfg.slug }) == cfg else { return LibraryBusy.unregistered }
        if let busy = busyReason(slug: cfg.slug) { return busy }
        if hasActiveOwnedControlRuntime(for: cfg.slug) { return LibraryBusy.runtime }
        return hasMismatchedCachedControlConfiguration(for: cfg) ? mismatchedCachedControlRefusal(for: cfg) : nil
    }

    func libraryWorkRefusal(
        slug: String, excluding reservation: (HvfEngineSession, UUID)? = nil, excludingInstall: NativeInstallOperation? = nil,
        excludingWindowsInstall: HvfWindowsInstallSession? = nil,
        owns: @MainActor (LibraryModel, VMConfig) -> Bool
    ) -> String? {
        guard let config = vms.first(where: { $0.slug == slug }) else { return LibraryBusy.unregistered }
        guard owns(self, config) else {
            return "VM 제어 대상이 변경되었습니다. 목록을 새로고침하고 VM 화면을 다시 여세요."
        }
        return genericRetainedWorkRefusal(for: config) ?? busyReason(slug: slug, excluding: reservation,
            excludingInstall: excludingInstall, excludingWindowsInstall: excludingWindowsInstall)
    }
}
