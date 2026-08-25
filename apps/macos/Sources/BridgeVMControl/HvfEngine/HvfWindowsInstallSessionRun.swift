import Foundation

@MainActor
extension HvfWindowsInstallSession {
    func run() async {
        guard await prepareMedia() else { return }
        if plan.request.injectViogpu3d {
            guard await injectVerifiedPackage() else { return }
        }
        stage = .finalizing
        do {
            try finalizeMedia()
            HvfWindowsInjectionWorkspace.cleanup(plan)
        } catch {
            stage = .failed("설치 결과 반영 실패: \(error.localizedDescription)")
            return
        }
        stage = .done
        appendLog(plan.importsExistingMedia
            ? "가져온 Windows와 kernel-policy 드라이버 준비가 완료되었습니다."
            : "Windows 설치가 완료되었습니다.")
        onCompleted?()
    }

    private func prepareMedia() async -> Bool {
        if plan.importsExistingMedia {
            stage = .preparingImportedMedia
            do { try HvfWindowsInjectionWorkspace.cloneImportedMedia(plan) }
            catch {
                failUnlessCancelled("가져온 디스크/vars의 안전 복제에 실패했습니다.")
                return false
            }
            return true
        }
        if !plan.sourceImageIsCached {
            stage = .preparingSource
            let build = plan.sourceBuildCommand()
            guard await runProcess(
                arguments: build.arguments, extraEnvironment: build.environment,
                progressLog: nil) else {
                try? FileManager.default.removeItem(atPath: plan.sourceImagePath)
                failUnlessCancelled("설치 소스 생성이 실패했습니다.")
                return false
            }
        } else { appendLog("설치 소스 캐시 재사용: \(plan.sourceImagePath)") }
        stage = .installing
        try? FileManager.default.createDirectory(
            atPath: plan.tmpEvidenceDir, withIntermediateDirectories: true)
        let log = URL(fileURLWithPath: plan.tmpEvidenceDir).appendingPathComponent("run.log")
        guard await runProcess(
            arguments: plan.installCommand(), extraEnvironment: [:], progressLog: log) else {
            failUnlessCancelled("Windows 무인 설치가 실패했습니다. 로그: \(log.path)")
            return false
        }
        return true
    }

    private func injectVerifiedPackage() async -> Bool {
        guard plan.injectionValidationError() == nil else {
            failUnlessCancelled("bundle-private kernel-policy snapshot 재검증이 실패했습니다.")
            return false
        }
        do { try HvfWindowsInjectionWorkspace.reset(plan) }
        catch {
            failUnlessCancelled("private 주입 작업 디렉터리를 안전하게 만들지 못했습니다.")
            return false
        }
        guard let command = plan.injectorBuildCommand() else {
            failUnlessCancelled("고정 wimlib-imagex 또는 kernel-policy builder를 찾지 못했습니다.")
            return false
        }
        stage = .buildingInjector
        guard await runProcess(arguments: command, extraEnvironment: [:], progressLog: nil) else {
            failUnlessCancelled("검증 snapshot 전용 WinPE 인젝터 생성이 실패했습니다.")
            return false
        }
        stage = .injecting
        let runLog = URL(fileURLWithPath: plan.injectionEvidencePath)
            .appendingPathComponent("run.log")
        guard await runProcess(
            arguments: plan.injectionCommand(), extraEnvironment: [:], progressLog: runLog),
              plan.injectionBootWasObserved(), plan.injectionValidationError() == nil else {
            failUnlessCancelled("봉인된 인젝터 부팅 또는 주입 후 snapshot 재검증이 실패했습니다.")
            return false
        }
        return true
    }

    private func failUnlessCancelled(_ message: String) {
        if cancelled {
            HvfWindowsInjectionWorkspace.cleanup(plan)
            stage = .failed("설치가 취소되었습니다.")
        } else { stage = .failed(message) }
    }

    private func finalizeMedia() throws {
        let fm = FileManager.default
        // Finish fallible mutation before canonical names change.
        let secureBootReceipt = try HvfWindowsBootSeed.seedFile(
            varsPath: plan.tmpVarsPath, diskPath: plan.tmpTargetPath)
        if plan.request.injectViogpu3d {
            try HvfWindowsInjectionWorkspace.retainEvidence(plan)
        }
        for sub in ["disks", "metadata", "logs/hvf"] {
            try fm.createDirectory(
                at: URL(fileURLWithPath: plan.bundlePath).appendingPathComponent(sub),
                withIntermediateDirectories: true)
        }
        try HvfWindowsMediaPublicationTransaction.publish(
            disk: (plan.bundleDiskPath, plan.tmpTargetPath),
            vars: (plan.bundleVarsPath, plan.tmpVarsPath))
        let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(secureBootReceipt).write(
            to: URL(fileURLWithPath: plan.bundlePath)
                .appendingPathComponent("metadata/secure-boot-provisioning.json"),
            options: [.atomic])
        appendLog("UEFI 부팅 항목과 Microsoft-only Secure Boot 키를 검증·시드했습니다.")
        let installLog = URL(fileURLWithPath: plan.tmpEvidenceDir).appendingPathComponent("run.log")
        if fm.fileExists(atPath: installLog.path) {
            try? fm.removeItem(atPath: plan.bundleInstallLogPath)
            try? fm.copyItem(atPath: installLog.path, toPath: plan.bundleInstallLogPath)
        }
        let controlPath = "\(plan.bundlePath)/metadata/hvf.ctl"
        if !fm.fileExists(atPath: controlPath) { fm.createFile(atPath: controlPath, contents: nil) }
        let pending = URL(fileURLWithPath: plan.bundlePath)
            .appendingPathComponent(HvfWindowsInstallRequest.fileName)
        let done = URL(fileURLWithPath: plan.bundlePath)
            .appendingPathComponent(HvfWindowsInstallRequest.doneFileName)
        try? fm.removeItem(at: done)
        try? fm.moveItem(at: pending, to: done)
    }
}
