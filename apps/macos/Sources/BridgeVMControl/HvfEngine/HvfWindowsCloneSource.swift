import Foundation

enum HvfWindowsCloneSource {
    enum Phase { case admission, copy }

    static func refusal(for config: VMConfig, phase: Phase) -> String? {
        let bundle = URL(fileURLWithPath: config.bundlePath)
        let disks = bundle.appendingPathComponent("disks")
        let builtin = disks.appendingPathComponent("hvf-target.raw")
        let selected = URL(fileURLWithPath: config.diskPath ?? builtin.path)
        guard selected.resolvingSymlinksInPath().standardizedFileURL == builtin.resolvingSymlinksInPath().standardizedFileURL else {
            return "등록된 디스크가 번들 내부 복제 대상과 다릅니다. 선택한 디스크를 새 VM으로 가져온 뒤 복제하세요."
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
