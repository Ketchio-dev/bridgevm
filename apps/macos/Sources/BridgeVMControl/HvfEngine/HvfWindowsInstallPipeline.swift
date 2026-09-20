import Foundation

extension HvfWindowsInstallSession {
    func run() async {
        guard await continueFreshInstallation() else { return }
        let sourceLock: HvfWindowsInstallSourceLock
        do {
            sourceLock = try HvfWindowsInstallSourceLock(sourceImagePath: plan.sourceImagePath)
        } catch {
            finish(.failed(error.localizedDescription))
            return
        }
        defer { withExtendedLifetime(sourceLock) {} }
        let sourceVerified = await execution.verifySource(plan)
        guard !acknowledgeCancellation() else { return }
        guard await continueFreshInstallation() else { return }
        if !sourceVerified {
            transition(to: .preparingSource)
            guard !acknowledgeCancellation() else { return }
            let build = plan.sourceBuildCommand()
            guard await runProcess(arguments: build.arguments, extraEnvironment: build.environment,
                                   progressLog: nil) else {
                try? FileManager.default.removeItem(atPath: plan.sourceImagePath)
                failUnlessCancelled("설치 소스 생성이 실패했습니다.")
                return
            }
            guard !acknowledgeCancellation(cleanup: true) else { return }
        } else {
            appendLog("설치 소스 캐시 재사용: \(plan.sourceImagePath)")
        }

        transition(to: .installing)
        guard !acknowledgeCancellation(cleanup: true) else { return }
        guard await continueFreshInstallation() else { return }
        do {
            try execution.prepareMedia(plan)
        } catch {
            failUnlessCancelled("번들된 UEFI vars 시드를 준비하지 못했습니다.")
            return
        }
        let installLog = URL(fileURLWithPath: plan.tmpEvidenceDir).appendingPathComponent("run.log")
        guard await runProcess(arguments: plan.installCommand(), extraEnvironment: [:],
                               progressLog: installLog) else {
            failUnlessCancelled("Windows 무인 설치가 실패했습니다. 로그: \(plan.tmpEvidenceDir)/run.log")
            return
        }
        guard !acknowledgeCancellation(cleanup: true) else { return }
        guard await continueFreshInstallation() else { return }

        // Durable publication has no rollback-on-cancel promise, including reentrant observers.
        commitStarted = true
        transition(to: .finalizing)
        do {
            try await execution.finalize(plan)
            appendLog("UEFI 부팅 항목과 Microsoft-only Secure Boot 키를 검증·시드했습니다.")
        } catch {
            finish(.failed("설치 결과 반영 실패: \(error.localizedDescription)"))
            return
        }
        completeInstallation()
    }
}
