import SwiftUI
#if canImport(AppKit)
import AppKit
import UniformTypeIdentifiers
#endif

extension HvfEngineView {
    func beginVTPMConfirmation(_ kind: HvfRuntimeMutationKind) {
        guard let reservation = session.reserveRuntimeMutation(kind: kind) else { return }
        vtpmMutationReservation = reservation
        if kind == .vtpmRecoveryRestore { confirmVTPMRestore = true }
        else { confirmVTPMReset = true }
    }
    func releaseVTPMReservation() {
        guard let reservation = vtpmMutationReservation else { return }
        vtpmMutationReservation = nil
        session.finishRuntimeMutation(reservation)
    }
    func exportVTPMRecovery() {
        #if canImport(AppKit)
        guard let reservation = session.reserveRuntimeMutation(kind: .vtpmRecoveryExport) else { return }
        defer { session.finishRuntimeMutation(reservation) }
        guard let keyID = reservation.configuration.vtpmKeyID,
              let statePath = reservation.configuration.vtpmStateDir else { return }
        let panel = NSSavePanel()
        panel.nameFieldStringValue = "\(keyID).bridgevm-vtpm-recovery.json"
        panel.allowedContentTypes = [.json]
        guard panel.runModal() == .OK, let destination = panel.url else { return }
        guard session.validateRuntimeMutation(reservation) else { return }
        do {
            try reservation.checkEffect()
            let lifecycle = VTPMIdentityLifecycle(keyStore: KeychainVTPMStateKeyStore())
            let result = try lifecycle.exportRecovery(
                stableVMID: keyID,
                stateDirectory: URL(fileURLWithPath: statePath, isDirectory: true),
                destination: destination
            )
            vtpmRecoveryCode = result.recoveryCode
            vtpmLifecycleMessage = "복구 패키지를 저장했습니다. 상태 지문: \(result.stateFingerprint)"
        } catch {
            vtpmLifecycleError = error.localizedDescription
        }
        #endif
    }

    func chooseVTPMRecoveryPackage() {
        #if canImport(AppKit)
        guard let reservation = session.reserveRuntimeMutation(kind: .vtpmRecoveryRestore) else { return }
        defer { session.finishRuntimeMutation(reservation) }
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.allowedContentTypes = [.json]
        if panel.runModal() == .OK, let url = panel.url {
            vtpmRecoveryPackagePath = url.path
            vtpmLifecycleMessage = nil
        }
        #endif
    }

    func restoreVTPMRecovery() {
        guard let reservation = vtpmMutationReservation, reservation.kind == .vtpmRecoveryRestore else { return }
        vtpmMutationReservation = nil // The executing operation now owns the reservation through return.
        defer { session.finishRuntimeMutation(reservation) }
        guard session.validateRuntimeMutation(reservation),
              let keyID = reservation.configuration.vtpmKeyID,
              let statePath = reservation.configuration.vtpmStateDir else { return }
        do {
            try reservation.checkEffect()
            let lifecycle = VTPMIdentityLifecycle(keyStore: KeychainVTPMStateKeyStore())
            try lifecycle.restoreRecovery(
                stableVMID: keyID,
                stateDirectory: URL(fileURLWithPath: statePath, isDirectory: true),
                packageURL: URL(fileURLWithPath: vtpmRecoveryPackagePath),
                recoveryCode: vtpmRecoveryCodeInput
            )
            vtpmRecoveryCodeInput = ""
            vtpmLifecycleMessage = "VM ID와 상태 지문을 검증하고 vTPM 키를 Keychain에 복원했습니다."
        } catch {
            vtpmLifecycleError = error.localizedDescription
        }
    }

    func resetVTPMIdentity() {
        guard let reservation = vtpmMutationReservation, reservation.kind == .vtpmReset else { return }
        vtpmMutationReservation = nil // The executing operation now owns the reservation through return.
        defer { session.finishRuntimeMutation(reservation) }
        guard session.validateRuntimeMutation(reservation),
              let keyID = reservation.configuration.vtpmKeyID,
              let statePath = reservation.configuration.vtpmStateDir else { return }
        do {
            try reservation.checkEffect()
            let lifecycle = VTPMIdentityLifecycle(keyStore: KeychainVTPMStateKeyStore())
            let result = try lifecycle.resetIdentity(
                stableVMID: keyID,
                stateDirectory: URL(fileURLWithPath: statePath, isDirectory: true)
            )
            vtpmLifecycleMessage = result.archivedStatePath.map {
                "새 TPM ID로 전환했습니다. 이전 상태: \($0) · 영수증: \(result.receiptPath)"
            } ?? "새 TPM ID로 전환했습니다. 영수증: \(result.receiptPath)"
        } catch {
            vtpmLifecycleError = error.localizedDescription
        }
    }

}
