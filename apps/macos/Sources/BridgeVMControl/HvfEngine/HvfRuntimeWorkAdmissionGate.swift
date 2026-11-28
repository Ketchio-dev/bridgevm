/// Admission may synchronously publish a refusal. Reentry must not invoke it again.
@MainActor
final class HvfRuntimeWorkAdmissionGate {
    private(set) var isChecking = false

    func check(_ admission: LibraryWorkAdmission?, reportRefusal: Bool) -> String? {
        guard !isChecking else { return "BridgeVM is still checking this VM. Try again in a moment." }
        isChecking = true
        defer { isChecking = false }
        return admission?(reportRefusal)
    }
}
