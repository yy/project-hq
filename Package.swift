// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "HQDesktop",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "HQ", targets: ["HQDesktop"]),
        .executable(name: "HQSyncFile", targets: ["HQSyncFile"])
    ],
    targets: [
        .target(name: "MobileFiles", path: "apple/MobileFiles"),
        .executableTarget(name: "HQSyncFile", path: "macos/HQSyncFile"),
        .testTarget(name: "HQSyncFileTests", dependencies: ["HQSyncFile"], path: "macos/SyncTests"),
        .executableTarget(
            name: "HQDesktop",
            dependencies: ["MobileFiles"],
            path: "macos/HQDesktop"
        ),
        .testTarget(
            name: "HQDesktopTests",
            dependencies: ["HQDesktop"],
            path: "macos/Tests"
        )
    ]
)
