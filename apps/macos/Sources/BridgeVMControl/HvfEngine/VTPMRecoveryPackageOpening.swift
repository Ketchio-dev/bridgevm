import CryptoKit
import Foundation

extension VTPMIdentityLifecycle {
    func importedRecoveryKey(stateDirectory: URL, packageURL: URL, recoveryCode: String) throws -> Data {
        var secret = try Self.decodeRecoveryCode(recoveryCode)
        defer { secret.resetBytes(in: secret.indices) }
        let package: VTPMRecoveryPackage
        do { package = try JSONDecoder().decode(VTPMRecoveryPackage.self,
            from: Data(contentsOf: packageURL, options: [.mappedIfSafe])) }
        catch { throw VTPMIdentityLifecycleError.invalidRecoveryPackage }
        guard package.format == Self.recoveryFormat,
              try stateFingerprint(at: stateDirectory) == package.stateFingerprint,
              let nonceData = Data(base64Encoded: package.nonce),
              let ciphertext = Data(base64Encoded: package.ciphertext),
              let tag = Data(base64Encoded: package.tag),
              let nonce = try? AES.GCM.Nonce(data: nonceData),
              let box = try? AES.GCM.SealedBox(nonce: nonce, ciphertext: ciphertext, tag: tag)
        else { throw VTPMIdentityLifecycleError.invalidRecoveryPackage }
        let aad = Self.associatedData(stableVMID: package.stableVMID,
            createdAt: package.createdAt, stateFingerprint: package.stateFingerprint)
        let key: Data
        do { key = try AES.GCM.open(box, using: SymmetricKey(data: secret), authenticating: aad) }
        catch { throw VTPMIdentityLifecycleError.invalidRecoveryPackage }
        guard key.count == KeychainVTPMStateKeyStore.keyLength else {
            throw VTPMIdentityLifecycleError.invalidRecoveryPackage
        }
        return key
    }
}
