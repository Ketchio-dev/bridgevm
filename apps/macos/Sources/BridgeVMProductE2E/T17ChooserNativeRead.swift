import Foundation

enum T17ChooserNativeRead {
    static func perform<Value>(budget: T17ChooserNativeBudget,
                              operation: T17ChooserNativeBudget.Operation,
                              read: () throws -> Value) throws -> Value {
        try budget.check(operation)
        let value: Value
        do { value = try read() }
        catch {
            if let blocker = error as? T17Blocker, T17ChooserNativeBudget.isDeadlineFailure(blocker) {
                throw T17FileChooser.failure("native chooser deadline exhausted; operation=\(operation.rawValue); nested{" +
                    String(blocker.detail.prefix(240)) + "}" + knownAXError(blocker.detail))
            }
            do { try budget.check(operation, boundary: .returned) }
            catch let late as T17Blocker where T17ChooserNativeBudget.isDeadlineFailure(late) {
                let original = (error as? T17Blocker)?.detail ?? String(describing: error)
                throw T17FileChooser.failure(late.detail + "; original{" +
                    String(original.prefix(240)) + "}" + knownAXError(original))
            }
            throw error // Preserve the original object and text when the read failed within budget.
        }
        try budget.check(operation, boundary: .returned)
        return value
    }
    // Retain an already recorded AX code even when outer wrapper clipping removes its text.
    private static func knownAXError(_ detail: String) -> String {
        guard let field = detail.range(of: "ax_error=", options: .backwards) else { return "" }
        let digits = detail[field.upperBound...].prefix(12).prefix { $0 == "-" || $0.isNumber }
        guard let value = Int32(digits) else { return "" }
        return "; ax_error=\(value)"
    }
}
