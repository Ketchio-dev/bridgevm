// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "BridgeVMApp",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "BridgeVMApp", targets: ["BridgeVMApp"]),
        .executable(name: "AppleVzRunner", targets: ["AppleVzRunner"]),
        .executable(name: "BridgeVMControl", targets: ["BridgeVMControl"]),
        .executable(name: "BridgeVMProductE2E", targets: ["BridgeVMProductE2E"]),
    ],
    targets: [
        .executableTarget(
            name: "BridgeVMApp",
            path: "Sources/BridgeVMApp"
        ),
        .executableTarget(
            name: "BridgeVMControl",
            path: "Sources/BridgeVMControl",
            resources: [
                .copy("Resources/windows-boot-seed-vars.fd.gz"),
                .copy("Resources/secureboot-microsoft-windows-transition-aarch64-v1.6.5.json")
            ]
        ),
        .executableTarget(name: "BridgeVMProductE2E", path: "Sources/BridgeVMProductE2E"),
        .target(
            name: "AppleVzRunnerCore",
            path: "Sources/AppleVzRunnerCore"
        ),
        .executableTarget(
            name: "AppleVzRunner",
            dependencies: ["AppleVzRunnerCore"],
            path: "Sources/AppleVzRunner"
        ),
        .testTarget(
            name: "BridgeVMAppTests",
            dependencies: ["BridgeVMApp"],
            path: "Tests/BridgeVMAppTests"
        ),
        .testTarget(
            name: "AppleVzRunnerTests",
            dependencies: ["AppleVzRunnerCore"],
            path: "Tests/AppleVzRunnerTests"
        ),
        .testTarget(
            name: "BridgeVMControlTests",
            dependencies: ["BridgeVMControl"],
            path: "Tests/BridgeVMControlTests"
        ),
        .testTarget(name: "BridgeVMProductE2ETests", dependencies: ["BridgeVMProductE2E"], path: "Tests/BridgeVMProductE2ETests")
    ]
)
