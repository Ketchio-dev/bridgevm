import Foundation

enum T17CreatedVMIdentity {
    static func mismatches(
        in object: [String: Any], name: String, id: String, bundlePath: String
    ) -> [String] {
        [
            ("name", object["name"] as? String == name),
            ("id", object["id"] as? String == id),
            ("installPending", object["installPending"] as? Bool == true),
            ("bundlePath", object["bundlePath"] as? String == bundlePath),
        ].compactMap { field, matches in matches ? nil : field }
    }

    static func verify(
        _ object: [String: Any], name: String, id: String, bundlePath: String
    ) throws {
        let fields = mismatches(in: object, name: name, id: id, bundlePath: bundlePath)
        guard fields.isEmpty else {
            throw T17Blocker(
                code: "vm-creation-failed",
                detail: "UI-created VM config identity mismatch: \(fields.joined(separator: ","))")
        }
    }
}
