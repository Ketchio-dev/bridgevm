import Foundation

/// A nonblocking import notice kept with the destination, including after publication/read-back.
enum HvfWindowsImportNotice {
    static let noESP = "이 디스크에는 UEFI 부팅 파티션이 없어 시작되지 않을 수 있습니다."
    static let varsHelp = "다른 QEMU 기반 환경에서 디스크와 함께 받은 UEFI vars 파일이 있고 BitLocker 또는 사용자 지정 부팅 항목을 사용한다면 선택하세요."
    static let bitLockerHelp = "다른 컴퓨터의 TPM으로 보호된 BitLocker 디스크는 이 VM의 vTPM이 새로 생성되므로 복구 키를 요구합니다."

    private static func url(nextTo vars: URL) -> URL {
        vars.deletingLastPathComponent().appendingPathComponent("hvf-import-notice.txt")
    }

    static func write(nextTo vars: URL) throws {
        try Data(noESP.utf8).write(to: url(nextTo: vars), options: .atomic)
    }

    static func read(bundlePath: String) -> String? {
        let vars = URL(fileURLWithPath: bundlePath).appendingPathComponent("metadata/hvf-vars.fd")
        return try? String(contentsOf: url(nextTo: vars), encoding: .utf8)
    }
}
