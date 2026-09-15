@MainActor
struct HvfRuntimeSessionRecord {
    let sourceConfig: VMConfig?
    let session: HvfEngineSession

    func retainedControl(excludingSlugs: Set<String>) -> LibraryRetainedControlDescriptor? {
        guard let sourceConfig, !excludingSlugs.contains(sourceConfig.slug),
              session.connectionState != .stopped else { return nil }
        return .runtime(config: sourceConfig, session: session)
    }
}

@MainActor
struct HvfWindowsInstallSessionRecord {
    let sourceConfig: VMConfig
    let request: HvfWindowsInstallRequest
    let session: HvfWindowsInstallSession

    func retainedControl(excludingSlugs: Set<String>) -> LibraryRetainedControlDescriptor? {
        guard !excludingSlugs.contains(sourceConfig.slug), session.isRunning else { return nil }
        return .install(config: sourceConfig, session: session)
    }
}
