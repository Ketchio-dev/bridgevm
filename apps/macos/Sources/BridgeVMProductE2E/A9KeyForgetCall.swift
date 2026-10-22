import Foundation

/// One product `--vtpm-lifecycle forget-import` call, judged only by the product's own words: exit 0 with exactly
/// one proof line for the slug, or exit 1 with its missing-key refusal. Anything else, including a call still
/// running at the bound (a Keychain prompt, say), is a failure, so the lane's cleanup stays unverified.
enum A9KeyForgetCall {
    enum Outcome: Equatable { case forgotten, absent, failed }

    /// VTPMStateSecurityError.missingKeyForExistingState as VTPMImportCleanupCommand prints it.
    static let missingKeyLine = "ERROR: 기존 vTPM 상태의 Keychain 키가 없습니다. 상태를 덮어쓰지 않았습니다. "
        + "복구 키를 복원하거나 명시적으로 TPM을 재설정하세요."
    static let outputLimit = 65_536

    static func run(executable: String, arguments: [String], slug: String, timeout: TimeInterval) -> Outcome {
        let process = Process(), output = Pipe(), errors = Pipe(), ended = DispatchSemaphore(value: 0)
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        process.standardOutput = output; process.standardError = errors
        process.terminationHandler = { _ in ended.signal() }
        do { try process.run() } catch { return .failed }
        guard ended.wait(timeout: .now() + timeout) == .success else {
            process.terminate(); _ = ended.wait(timeout: .now() + 5); return .failed
        }
        guard process.terminationReason == .exit else { return .failed }
        let proofs = lines(output).filter { $0.hasPrefix("forgot_imported_key=") }
        if process.terminationStatus == 0, proofs == ["forgot_imported_key=\(slug)"] { return .forgotten }
        if process.terminationStatus == 1, proofs.isEmpty, lines(errors).contains(missingKeyLine) { return .absent }
        return .failed
    }

    private static func lines(_ pipe: Pipe) -> [String] {
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        guard data.count <= outputLimit else { return [] }
        return String(decoding: data, as: UTF8.self).split(whereSeparator: \.isNewline).map(String.init)
    }
}
