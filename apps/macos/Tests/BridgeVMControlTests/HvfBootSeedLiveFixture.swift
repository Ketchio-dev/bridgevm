import Foundation
import XCTest

enum HvfBootSeedLiveFixture {
    static func prepare(environment: [String: String] = ProcessInfo.processInfo.environment) throws -> (String, String) {
        let keys = ["BRIDGEVM_BOOT_SEED_LIVE_DISK", "BRIDGEVM_BOOT_SEED_LIVE_TEMPLATE", "BRIDGEVM_BOOT_SEED_LIVE_OUTPUT"]
        if keys.allSatisfy({ environment[$0] == nil }) {
            throw XCTSkip("live seed fixture requires explicit BRIDGEVM_BOOT_SEED_LIVE_* paths")
        }
        guard let disk = environment[keys[0]], let template = environment[keys[1]], let output = environment[keys[2]],
              [disk, template, output].allSatisfy({ $0.hasPrefix("/") }) else {
            throw CocoaError(.fileReadInvalidFileName)
        }
        let fm = FileManager.default
        for input in [disk, template] {
            guard try fm.attributesOfItem(atPath: input)[.type] as? FileAttributeType == .typeRegular,
                  fm.isReadableFile(atPath: input) else { throw CocoaError(.fileReadNoPermission) }
        }
        guard !fm.fileExists(atPath: output),
              (try? fm.destinationOfSymbolicLink(atPath: output)) == nil else {
            throw CocoaError(.fileWriteFileExists)
        }
        try fm.copyItem(atPath: template, toPath: output)
        return (disk, output)
    }
}
