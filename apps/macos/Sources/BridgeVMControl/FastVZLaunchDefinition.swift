import Foundation

struct FastVZLaunchDefinition {
    let config: VMConfig

    var process: GenericOwnedProcessSpec {
        .init(executable: config.runnerPath,
              arguments: ["--display", "--display-width", String(config.displayWidth),
                          "--display-height", String(config.displayHeight), "--allow-real-vz-start",
                          "--handoff-json", config.handoffPath],
              environmentOverrides: ["BRIDGEVM_APPLE_VZ_ALLOW_REAL_START": "1"])
    }

    var command: String {
        "BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1 nohup \(Shell.shellCommand(process.executable, process.arguments)) >/dev/null 2>&1 &"
    }

    func validatedPlan() -> GenericOwnedLaunchPlan? {
        guard FileManager.default.isExecutableFile(atPath: config.runnerPath),
              FileManager.default.isReadableFile(atPath: config.handoffPath),
              config.displayWidth > 0, config.displayHeight > 0 else { return nil }
        return .init(primary: process)
    }
}
