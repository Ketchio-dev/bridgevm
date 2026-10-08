import SwiftUI

extension CreateVMSheet {
    func completeCreation(_ outcome: HVFCreationOutcome?) {
        working = false
        let result = creationState.complete(outcome, adopt: library.add)
        creationFailureCode = result.code
        error = result.message
        if result.dismiss { dismiss() }
    }
}
