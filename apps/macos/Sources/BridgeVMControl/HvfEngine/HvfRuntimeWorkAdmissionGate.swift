/// Admission may synchronously publish a refusal. Reentry must not invoke it again.
@MainActor
final class HvfRuntimeWorkAdmissionGate {
    private(set) var isChecking = false

    func check(_ admission: LibraryWorkAdmission?, reportRefusal: Bool) -> String? {
        guard !isChecking else { return "Runtime admission is already being checked" }
        isChecking = true
        defer { isChecking = false }
        return admission?(reportRefusal)
    }
}
