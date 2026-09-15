import Foundation

extension ControlModel {
    func runTerminalCommand() {
        guard !busy, !lifecycleBusy, running, backend.supportsGuestCommands else { return }
        let cmd = terminalInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cmd.isEmpty else { return }
        terminalLog = Self.boundedLog(terminalLog + "dev@guest$ \(cmd)\n", limit: Self.terminalLogCharacterLimit)
        terminalInput = ""
        busy = true
        let backend = self.backend
        Task.detached {
            let r = backend.runInGuest(cmd)
            await MainActor.run {
                var addition = r.output
                if !r.output.hasSuffix("\n") { addition += "\n" }
                if r.code != 0 { addition += "[exit \(r.code)]\n" }
                addition += "\n"
                self.terminalLog = Self.boundedLog(
                    self.terminalLog + addition,
                    limit: Self.terminalLogCharacterLimit
                )
                self.busy = false
            }
        }
    }

    func installPackages(_ packages: [String], label: String) {
        guard !busy, !lifecycleBusy, running, backend.supportsPackageInstall else { return }
        guard let command = Self.packageInstallCommand(packages) else {
            softwareLog = "설치 요청이 올바르지 않습니다. 패키지 이름을 확인해 주세요.\n"
            return
        }
        busy = true
        softwareLog = "\(label) 설치 중… (apt, 잠시 걸립니다)\n"
        let backend = self.backend
        Task.detached {
            let r = backend.runInGuest(command)
            await MainActor.run {
                let result = r.output + ((r.code == 0) ? "\n✅ 설치 완료: \(label)\n" : "\n❌ 실패 (exit \(r.code))\n")
                self.softwareLog = Self.boundedLog(
                    self.softwareLog + result,
                    limit: Self.softwareLogCharacterLimit
                )
                self.busy = false
            }
        }
    }

}
