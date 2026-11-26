import Foundation

enum HvfWindowsCloneSource {
    enum Phase { case admission, copy }

    static func refusal(for config: VMConfig, phase: Phase) -> String? {
        let bundle = URL(fileURLWithPath: config.bundlePath)
        let disks = bundle.appendingPathComponent("disks")
        let builtin = disks.appendingPathComponent("hvf-target.raw")
        let selected = URL(fileURLWithPath: config.diskPath ?? builtin.path)
        guard selected.resolvingSymlinksInPath().standardizedFileURL == builtin.resolvingSymlinksInPath().standardizedFileURL else {
            return "This VM uses a disk outside the copy selected for duplication. First import the active disk as a new VM, then duplicate it."
        }
        return HvfWindowsCloneMedia.refusal(bundle: bundle, phase: phase)
    }
}

extension LibraryModel {
    func cloneSourceRefusal(for config: VMConfig, action: LibraryFileAction) -> String? {
        guard action == .clone, config.engineKind == .hvfEngine else { return nil }
        return HvfWindowsCloneSource.refusal(for: config, phase: .admission)
    }
}
