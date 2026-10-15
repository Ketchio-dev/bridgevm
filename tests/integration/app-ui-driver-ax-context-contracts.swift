import Foundation
import Darwin

@main
enum AppUIDriverAXContextContracts {
    typealias T = AppUIDriverProtocolTestSupport
    static func run(cancel: Bool) throws {
        let original: Data
        let validate: (Data) throws -> Void
        if cancel {
            let request = try AppUIInstallCancelProtocolTestSupport.request()
            let reply = try AppUIInstallCancelProtocolTestSupport.reply(request, failure: .axFailure)
            original = try AppUIDriverCanonicalJSON.encode(reply)
            validate = { data in
                let value = try AppUIDriverCanonicalJSON.decode(data, as: AppUIInstallCancelReply.self)
                try AppUIInstallCancelValidation.reply(value, request: request,
                    session: AppUIInstallCancelProtocolTestSupport.session(), requestSHA256: reply.requestSHA256)
            }
        } else {
            let request = try T.request()
            let reply = try T.reply(request, failure: .axFailure)
            original = try AppUIDriverCanonicalJSON.encode(reply)
            validate = { data in
                let value = try AppUIDriverCanonicalJSON.decode(data, as: AppUIDriverReply.self)
                try AppUIDriverValidation.reply(value, request: request,
                    session: T.session(), requestSHA256: reply.requestSHA256)
            }
        }
        try validate(original)
        try T.check(!String(decoding: original, as: UTF8.self).contains("axContext"), "legacy refusal stays unchanged")
        let base = try JSONSerialization.jsonObject(with: original) as! [String: Any]
        func data(_ context: [String: String], _ change: (inout [String: Any]) -> Void = { _ in }) throws -> Data {
            var value = base
            value["axContext"] = context
            change(&value)
            return try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys, .withoutEscapingSlashes])
        }
        let context = ["api": "copyAttributeValues", "attribute": "AXWindows", "scope": "element"]
        try validate(data(context))
        try T.check(true, "bounded failure context round trips through actual reply validation")
        for (key, bad) in [("api", "private API text"), ("attribute", "private UI text"), ("scope", "private path")] {
            var wrong = context; wrong[key] = bad
            try T.refuses("unknown context vocabulary") { try validate(data(wrong)) }
        }
        var extra = context; extra["text"] = "private UI text"
        try T.refuses("additional context text") { try validate(data(extra)) }
        for field in ["axError", "failureCode"] {
            try T.refuses("context requires AX failure and raw error") {
                try validate(data(context) { $0.removeValue(forKey: field) })
            }
        }
        try T.refuses("zero is not an AX error") { try validate(data(context) { $0["axError"] = 0 }) }
        try T.refuses("non-AX refusal cannot carry context") { try validate(data(context) { $0["failureCode"] = "cancelled" }) }
        try T.refuses("otherwise valid success cannot carry context") {
            try validate(data(context) {
                $0["outcome"] = "observed"; $0.removeValue(forKey: "axError"); $0.removeValue(forKey: "failureCode")
                $0["values"] = cancel
                    ? ["installVisible": true, "startVisible": true, "startEnabled": true,
                       "cancelVisible": false, "cancelEnabled": false, "stage": "대기", "title": "설치 시작"]
                    : ["createVisible": true, "importVisible": true]
            })
        }
        var child = context; child["scope"] = "relatedElement"
        try T.refuses("child context only identifies child preparation") { try validate(data(child)) }
        for api in ["getPID", "setMessagingTimeout"] {
            child["api"] = api
            try validate(data(child))
            try T.check(true, "child preparation context accepted")
        }
    }
    static func main() {
        do {
            try run(cancel: false); try run(cancel: true)
            print("PASS: \(T.checks) AX error context protocol checks; no GUI or permission query")
        } catch {
            FileHandle.standardError.write(Data("FAIL: AX error context protocol: \(error)\n".utf8))
            exit(1)
        }
    }
}
