import SwiftUI

extension CreateVMSheet {
    var creationFooter: some View {
        CreateVMCreationFooter(working: working, error: error, failureCode: creationFailureCode,
            canCreate: canCreate, cancel: { dismiss() }, create: create,
            recoveryAvailable: !creationState.permitsCreation, recover: recoverCreatedVM)
    }
}
