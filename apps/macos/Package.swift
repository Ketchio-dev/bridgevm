// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "BridgeVMApp",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "BridgeVMApp", targets: ["BridgeVMApp"]),
        .executable(name: "AppleVzRunner", targets: ["AppleVzRunner"]),
    ],
    targets: [
        .executableTarget(
            name: "BridgeVMApp",
            path: "Sources/BridgeVMApp"
        ),
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
        )
    ]
)
