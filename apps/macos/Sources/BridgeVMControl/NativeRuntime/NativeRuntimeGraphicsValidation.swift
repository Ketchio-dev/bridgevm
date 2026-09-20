import Foundation

extension NativeRuntimeCodec {
    static func validateGraphicsMode(_ session: NativeRuntimeSessionObservation) throws {
        guard let mode = session.graphicsMode else { return }
        let valid: Bool
        switch session.ownership {
        case .owned:
            valid = mode == .basic3DOff || mode == .experimental3D
        case .attachedObservation:
            valid = mode == .unverified
        case .ownedExitObserved, .notObserved:
            valid = false
        }
        guard valid else { throw NativeRuntimeError.invalidMessage }
    }
}
