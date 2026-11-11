import Foundation

extension HvfLibraryLaunchContext {
    var readinessIssues: [HvfWindowsReadinessIssue] {
        var issues: [HvfWindowsReadinessIssue] = []
        if VMRelocationJournal.isPending(config, rootURL: rootURL) {
            issues.append(.init(code: "relocation-pending", scope: .launch,
                summary: "VM 이동 복구가 끝나지 않았습니다. 시작하기 전에 두 번들과 등록 정보를 확인하세요."))
        }
        if HvfWindowsInstallTransactionResidue.isPresent(config, rootURL: rootURL) {
            issues.append(.init(code: "install-cleanup-pending", scope: .launch,
                summary: "Windows 설치 정리가 끝나지 않았습니다. 라이브러리를 새로 고치고, 계속 막히면 보존된 설치 기록을 확인하세요. 다시 설치하지 마세요."))
        }
        return issues
    }
}
