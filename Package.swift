// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "HQDesktop",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "HQ", targets: ["HQDesktop"])
    ],
    targets: [
        .executableTarget(
            name: "HQDesktop",
            path: "macos/HQDesktop"
        )
    ]
)
