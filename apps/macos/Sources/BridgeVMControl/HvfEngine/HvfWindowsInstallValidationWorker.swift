import Foundation

enum HvfWindowsInstallValidationWorker {
    typealias Validator = @Sendable (HvfWindowsInstallPlan) -> String?

    static func validate(_ plan: HvfWindowsInstallPlan, using validator: @escaping Validator) async -> String? {
        await Task.detached(priority: .userInitiated) { validator(plan) }.value
    }
}
