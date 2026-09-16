import Foundation

struct NativeCLICreateWindowsOptions: Equatable {
    struct Resolution: Equatable {
        let width: Int
        let height: Int
        var text: String { "\(width)x\(height)" }
    }

    let name: String
    let isoPath: String
    let diskGiB: Int
    let memoryMiB: Int
    let cpuCount: Int
    let resolution: Resolution
    let networkEnabled: Bool

    static let diskChoices = [64, 96, 128, 256, 512]
    static let memoryChoices = [2_048, 4_096, 6_144, 8_192, 12_288, 16_384, 24_576, 32_768]
    static let resolutions = [
        "1280x800": Resolution(width: 1280, height: 800),
        "1440x900": Resolution(width: 1440, height: 900),
        "1920x1080": Resolution(width: 1920, height: 1080),
        "2560x1440": Resolution(width: 2560, height: 1440),
    ]
}
