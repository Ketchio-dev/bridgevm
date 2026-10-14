import Darwin
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
        // The copier preserves links inside its source. Resolving only the final
        // disk would silently admit a copied link to the original writable media.
        for (url, kind) in [(bundle, mode_t(S_IFDIR)), (disks, mode_t(S_IFDIR)), (builtin, mode_t(S_IFREG))] {
            var info = stat()
            if lstat(url.path, &info) == 0 {
                if info.st_mode & mode_t(S_IFMT) == kind { continue }
            } else if errno == ENOENT && phase == .admission {
                // Keep the existing UI scheduling policy for absent media; the
                // actual copier requires the complete chain before reservation.
                continue
            }
            return "복제할 번들·disks 폴더와 디스크가 일반 파일 경로인지 확인하세요. 심볼릭 링크나 누락된 미디어는 새 VM으로 가져온 뒤 복제하세요."
        }
        return nil
    }
}

extension LibraryModel {
    func cloneSourceRefusal(for config: VMConfig, action: LibraryFileAction) -> String? {
        guard action == .clone, config.engineKind == .hvfEngine else { return nil }
        return HvfWindowsCloneSource.refusal(for: config, phase: .admission)
    }
}
