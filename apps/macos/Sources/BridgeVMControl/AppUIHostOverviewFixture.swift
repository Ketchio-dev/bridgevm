#if DEBUG && BRIDGEVM_APP_UI_HOST
import Foundation

@MainActor
extension AppUIHost {
    func seedOverviewMetadata() {
        library.vms = [("Amber", 4096), ("Indigo", 8192)].map { name, memory in
            VMConfig(id: "ui-" + name.lowercased(), name: name, displayName: name,
                backendKind: "hvf-engine", bootMode: "windows-hvf",
                bundlePath: root.appendingPathComponent(name + ".vmbridge").path,
                runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                leasesPath: "", guestName: "owned-ui-fixture", displayWidth: 1280, displayHeight: 800,
                memMiB: memory, cpuCount: 4)
        }
    }
}
#endif
