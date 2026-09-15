#if DEBUG && BRIDGEVM_APP_UI_HOST
@MainActor
enum AppUIHostPresentationMatrix {
    struct Variant: Equatable {
        let dark: Bool
        let minimum: Bool
        var name: String { "welcome-\(dark ? "dark" : "light")-\(minimum ? "minimum" : "default")" }
    }
    enum Outcome: String { case passed, mismatch }
    struct Row {
        let variant: Variant
        let outcome: Outcome
        let observation: [String: Any]
    }
    static let variants = [Variant(dark: false, minimum: false), Variant(dark: false, minimum: true),
                           Variant(dark: true, minimum: false), Variant(dark: true, minimum: true)]

    static func run(
        admission: () throws -> Void,
        present: (Variant) async throws -> Void,
        capture: (Variant) throws -> Void,
        mismatch: (AppUIHostPresentationMismatch) throws -> Void,
        observe: () -> [String: Any],
        persist: ([Row]) throws -> Void
    ) async throws {
        var rows: [Row] = []
        try Task.checkCancellation()
        try persist(rows)
        for variant in variants {
            try Task.checkCancellation()
            try admission()
            var presentationMismatch: AppUIHostPresentationMismatch?
            do { try await present(variant) }
            catch let error as AppUIHostPresentationMismatch { presentationMismatch = error }
            try Task.checkCancellation()
            try admission()
            if let error = presentationMismatch { try mismatch(error) }
            else { try capture(variant) }
            try Task.checkCancellation()
            rows.append(Row(variant: variant, outcome: presentationMismatch == nil ? .passed : .mismatch,
                            observation: observe()))
            try persist(rows)
        }
    }
}
#endif
