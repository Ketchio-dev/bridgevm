import Foundation

/// A nonblocking import notice kept with the destination, including after publication/read-back.
enum HvfWindowsImportNotice {
    static let noESP = "This disk has no boot partition, so Windows may not start."
    static let varsHelp = "If you use BitLocker or custom boot entries, include the matching UEFI variables file from your QEMU-based VM."
    static let bitLockerHelp = "If BitLocker protected this disk with another TPM, have your BitLocker recovery key ready."

    private static func url(nextTo vars: URL) -> URL {
        vars.deletingLastPathComponent().appendingPathComponent("hvf-import-notice.txt")
    }

    static func write(nextTo vars: URL) throws {
        try Data(noESP.utf8).write(to: url(nextTo: vars), options: .atomic)
    }

    static func read(bundlePath: String) -> String? {
        let vars = URL(fileURLWithPath: bundlePath).appendingPathComponent("metadata/hvf-vars.fd")
        let notice = try? String(contentsOf: url(nextTo: vars), encoding: .utf8)
        // Older imports stored the missing-partition notice as display text.
        let legacy = ["이 디스크에는 UEFI 부팅 파티션이 없어 시작되지 않을 수 있습니다.",
                      "This disk has no UEFI boot partition. Windows may fail to start."]
        return notice.map(legacy.contains) == true ? noESP : notice
    }
}
