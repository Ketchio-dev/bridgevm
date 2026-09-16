#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import CoreFoundation
import Foundation

extension AppUIDriverMailbox {
    func readHostCompletion() throws -> (pid: Int32, success: Bool)? {
        let observations = try root.child("host-observations")
        guard let data = try observations.read("host-completion.json") else { return nil }
        guard let session = try readSession() else { throw AppUIDriverFailure.invalidSession }
        let result = try AppUIDriverHostCompletion.decode(data)
        guard result.pid == session.session.host.pid else { throw AppUIDriverFailure.identityMismatch }
        return result
    }
}

enum AppUIDriverHostCompletion {
    static func decode(_ data: Data) throws -> (pid: Int32, success: Bool) {
        guard !data.isEmpty, data.count <= AppUIDriverConstants.maximumBytes,
              let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              Set(object.keys) == ["schema_version", "kind", "pid", "success", "cleanup_verified", "report_sha256", "failure"],
              try JSONSerialization.data(withJSONObject: object, options: [.prettyPrinted, .sortedKeys]) == data,
              integer(object["schema_version"]) == 1, object["kind"] as? String == "native-app-ui-host-completion",
              let pid = integer(object["pid"]), pid > 1, pid <= Int64(Int32.max),
              let success = boolean(object["success"]), let cleanup = boolean(object["cleanup_verified"]),
              let hash = object["report_sha256"] as? String, AppUIDriverValidation.hexDigest(hash),
              object["failure"] is NSNull || object["failure"] is String,
              !success || (cleanup && object["failure"] is NSNull) else { throw AppUIDriverFailure.invalidFile }
        return (Int32(pid), success)
    }

    private static func boolean(_ value: Any?) -> Bool? {
        guard let number = value as? NSNumber, CFGetTypeID(number) == CFBooleanGetTypeID() else { return nil }
        return number.boolValue
    }

    private static func integer(_ value: Any?) -> Int64? {
        guard let number = value as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID(),
              number.doubleValue.isFinite, number.doubleValue == Double(number.int64Value) else { return nil }
        return number.int64Value
    }
}
#endif
