// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "WallctlApp",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "WallctlApp", targets: ["WallctlApp"]),
    ],
    targets: [
        .executableTarget(
            name: "WallctlApp",
            linkerSettings: [
                .linkedFramework("AVFoundation"),
                .linkedFramework("IOKit"),
            ]
        ),
        .testTarget(name: "WallctlAppTests", dependencies: ["WallctlApp"]),
    ]
)
