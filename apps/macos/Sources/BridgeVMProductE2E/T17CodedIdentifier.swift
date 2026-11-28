/// The app carries state the harness reads as an accessibility identifier suffix,
/// "<identifier>.<code>", because VoiceOver reads labels and values but never identifiers.
enum T17CodedIdentifier {
    static let coded = ["bridgevm.create.error", "bridgevm.windows.runtime.state",
                        "bridgevm.windows.runtime.start.failure"]

    /// The identifier a lookup matches, without its code.
    static func base(_ identifier: String) -> String { split(identifier)?.base ?? identifier }

    static func code(_ identifier: String?) -> String? { identifier.flatMap(split)?.code }

    private static func split(_ identifier: String) -> (base: String, code: String)? {
        coded.first { identifier.hasPrefix($0 + ".") }.map { ($0, String(identifier.dropFirst($0.count + 1))) }
    }
}
