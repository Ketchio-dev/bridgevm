import Foundation

/// Advanced > Resources for a Fast VZ VM. The runner validates the handoff and
/// configures VZ from the launch spec; the library card, workspace list and
/// native CLI read vm.json. All three must carry the same allocation.
///
/// Ordering: both bundle files are read, and must already hold a `resources`
/// object, before anything is written, so an unreadable bundle is refused with
/// no write. vm.json is saved next (to the injected library root), then the
/// launch spec, then the handoff. If a later write fails, the earlier ones are
/// restored best-effort and the call reports failure. This is compensation,
/// not a transaction: a crash, or a failed restore, inside that window can
/// still leave the three files disagreeing.
enum FastVZResourceUpdate {
    static func apply(_ config: VMConfig, memMiB: Int, cpu: Int, libraryRoot: URL) -> VMConfig? {
        var registered = config
        registered.id = config.slug
        // Only an unchanged, still-present registration is rewritten; never resurrect or revert one.
        guard VMResourceLimits.contains(memoryMiB: memMiB, cpu: cpu),
              (try? NativeLibraryReader.readConfig(rootURL: libraryRoot, id: config.slug)) == registered,
              let launch = JSONFile.loadDict(config.launchSpecPath),
              let handoff = JSONFile.loadDict(config.handoffPath),
              let newLaunch = updating(launch, memMiB: memMiB, cpu: cpu),
              let newHandoff = updating(handoff, memMiB: memMiB, cpu: cpu) else { return nil }
        var updated = registered
        updated.memMiB = memMiB
        updated.cpuCount = cpu
        let saved = VMLibrary.saveOutcome(updated, rootURL: libraryRoot)
        guard saved.isCommitted else {
            if case .publishedButUnsynced = saved { VMLibrary.save(config, rootURL: libraryRoot) }
            return nil
        }
        guard JSONFile.writeDict(newLaunch, to: config.launchSpecPath) else {
            VMLibrary.save(config, rootURL: libraryRoot)
            return nil
        }
        guard JSONFile.writeDict(newHandoff, to: config.handoffPath) else {
            JSONFile.writeDict(launch, to: config.launchSpecPath)
            VMLibrary.save(config, rootURL: libraryRoot)
            return nil
        }
        return updated
    }

    private static func updating(_ object: [String: Any], memMiB: Int, cpu: Int) -> [String: Any]? {
        guard var resources = object["resources"] as? [String: Any] else { return nil }
        resources["memory"] = String(memMiB)
        resources["cpu"] = String(cpu)
        resources["rationale"] = "Set from BridgeVM Control app."
        var updated = object
        updated["resources"] = resources
        return updated
    }
}
