import SwiftUI

extension CreateVMSheet {
    func recoverCreatedVM() {
        guard !working, !creationState.permitsCreation else { return }
        working = true
        let loaded = creationState.recover(adopt: library.add)
        working = false
        if loaded { dismiss() }
        else { error = creationState.recoveryFailureMessage }
    }
}
