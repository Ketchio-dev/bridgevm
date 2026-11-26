import Foundation

typealias LibraryActionJob = @Sendable () async -> Void
typealias LibraryActionScheduler = @MainActor (@escaping LibraryActionJob) -> Void

enum LibraryFileAction { case deletion, clone, move }

private enum LibraryBusy {
    static let unregistered = "This VM’s registration changed. Refresh the library and try again."
    static let fileAction = "This VM is being deleted, duplicated or moved. Try again when the operation finishes."
    static let install = "Windows is being installed. Finish or cancel installation before trying again."
    static let runtime = "The VM is running, starting or shutting down. Wait until it is fully stopped, then try again."
    static let control = "A VM control operation is in progress. Try again when it finishes."
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
            return "The selected VM changed. Refresh the library and reopen its window."
        }
        return genericRetainedWorkRefusal(for: config) ?? busyReason(slug: slug, excluding: reservation,
            excludingInstall: excludingInstall, excludingWindowsInstall: excludingWindowsInstall)
    }
}
