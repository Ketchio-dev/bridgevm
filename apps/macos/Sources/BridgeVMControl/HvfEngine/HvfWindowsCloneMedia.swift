import Darwin
import Foundation

enum HvfWindowsCloneMedia {
    static func refusal(bundle: URL, phase: HvfWindowsCloneSource.Phase) -> String? {
        // Recursive copying preserves links. Both writable media paths and
        // their internal parents must be ordinary entries before reservation.
        let entries: [(String, mode_t)] = [
            ("", mode_t(S_IFDIR)),
            ("disks", mode_t(S_IFDIR)),
            ("disks/hvf-target.raw", mode_t(S_IFREG)),
            ("metadata", mode_t(S_IFDIR)),
            ("metadata/hvf-vars.fd", mode_t(S_IFREG))
        ]
        for (relative, kind) in entries {
            let url = relative.isEmpty ? bundle : bundle.appendingPathComponent(relative)
            var info = stat()
            if lstat(url.path, &info) == 0 {
                if info.st_mode & mode_t(S_IFMT) == kind { continue }
            } else if errno == ENOENT && phase == .admission {
                // Preserve UI scheduling for absent media; actual copying
                // requires the complete chain before reserving a destination.
                continue
            }
            return "복제할 번들·미디어 폴더와 디스크·UEFI vars가 일반 파일 경로인지 확인하세요. 심볼릭 링크나 누락된 미디어는 새 VM으로 가져온 뒤 복제하세요."
        }
        return nil
    }
}
