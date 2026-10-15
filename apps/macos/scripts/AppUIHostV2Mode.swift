import AppKit
import Foundation

enum AppUIHostV2Mode {
    case overview, installCancel
    init(flag: String) throws {
        switch flag {
        case "--app-ui-supervisor-v2": self = .overview
        case "--app-ui-install-cancel-supervisor-v1": self = .installCancel
        default: throw AppUIDriverFailure.invalidRequest
        }
    }
    var driverFlag: String { self == .overview ? "--app-ui-ax-driver" : "--app-ui-install-cancel-driver-v1" }
    var receiptName: String { self == .overview ? "launcher-observations-v2.json" : "install-cancel-launcher-observations.json" }
    var receiptKind: String { self == .overview ? "native-app-ui-launcher-v2" : "native-app-ui-install-cancel-launcher-v1" }
    func environment(output: URL) -> [String: String] {
        var result = ["BRIDGEVM_APP_UI_HOST_MODE": "1", "BRIDGEVM_APP_UI_HOST_OUTPUT": output.path,
                      "BRIDGEVM_APP_UI_DRIVER_MODE": "1"]
        if self == .installCancel { result["BRIDGEVM_APP_UI_INSTALL_CANCEL_MODE"] = "1" }
        return result
    }
    @MainActor static func dispatch(arguments: [String], startedAt: Double) throws -> Int32? {
        if arguments.first == "--app-ui-ax-driver" { return AppUIDriver.run(arguments: arguments) }
        if arguments.first == "--app-ui-install-cancel-driver-v1" { return AppUIInstallCancelDriver.run(arguments: arguments) }
        guard let flag = arguments.first, (try? Self(flag: flag)) != nil else { return nil }
        return try AppUIHostV2Supervisor(paths: AppUIHostV2Paths(arguments: arguments), startedAt: startedAt).run()
    }
}
