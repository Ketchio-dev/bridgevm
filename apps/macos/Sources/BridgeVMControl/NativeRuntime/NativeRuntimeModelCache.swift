import Foundation

/// Admission must still identify the same library when the lazy app model is first made.
@MainActor
final class NativeRuntimeModelCache<Model> {
    private let owner: NativeRuntimeOwner
    private(set) var retainedValue: Model?

    init(owner: NativeRuntimeOwner) { self.owner = owner }

    func model(make: () -> Model) throws -> Model {
        if let retainedValue { return retainedValue }
        try owner.validateCurrentOwnership()
        let value = make()
        retainedValue = value
        return value
    }
}
