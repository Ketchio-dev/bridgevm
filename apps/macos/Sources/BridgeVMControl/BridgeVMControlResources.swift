import Foundation

enum BridgeVMControlResources {
    static func url(forResource name: String, withExtension extensionName: String) -> URL? {
        let application = Bundle.main
        if application.bundleURL.pathExtension.lowercased() == "app" {
            // Packaged applications use their signed standard resource layout.
            // Missing resources must not fall through to SwiftPM's build path.
            guard let resources = application.resourceURL?.appendingPathComponent("BridgeVMApp_BridgeVMControl.bundle"),
                  let bundle = Bundle(url: resources) else { return nil }
            return bundle.url(forResource: name, withExtension: extensionName)
        }
        // Preserve SwiftPM developer executables and XCTest resource discovery.
        return Bundle.module.url(forResource: name, withExtension: extensionName)
    }
}
