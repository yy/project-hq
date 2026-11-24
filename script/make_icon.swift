#!/usr/bin/env swift
import Foundation

func run(_ executable: String, _ arguments: [String]) throws {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: executable)
    process.arguments = arguments
    try process.run()
    process.waitUntilExit()
    guard process.terminationStatus == 0 else {
        throw NSError(domain: "HQIcon", code: Int(process.terminationStatus), userInfo: [NSLocalizedDescriptionKey: "Icon export failed: \(executable)"])
    }
}

let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
let assetDir = root.appendingPathComponent("macos/Assets")
let source = assetDir.appendingPathComponent("AppIcon-1024.png")
let iconset = assetDir.appendingPathComponent("AppIcon.iconset")
try FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)
let xcode = Process()
let output = Pipe()
xcode.executableURL = URL(fileURLWithPath: "/usr/bin/xcode-select")
xcode.arguments = ["-p"]
xcode.standardOutput = output
try xcode.run()
let developer = String(decoding: output.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
xcode.waitUntilExit()
let ictool = URL(fileURLWithPath: developer).deletingLastPathComponent()
    .appendingPathComponent("Applications/Icon Composer.app/Contents/Executables/ictool")
try run(ictool.path, [root.appendingPathComponent("assets/AppIcon.icon").path,
                     "--export-image", "--output-file", source.path, "--platform", "macOS",
                     "--rendition", "Default", "--width", "1024", "--height", "1024", "--scale", "1"])
for points in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let suffix = scale == 2 ? "@2x" : ""
        let name = "icon_\(points)x\(points)\(suffix).png"
        let pixels = String(points * scale)
        try run("/usr/bin/sips", ["-z", pixels, pixels, source.path, "--out", iconset.appendingPathComponent(name).path])
    }
}
try run("/usr/bin/iconutil", ["-c", "icns", iconset.path, "-o", assetDir.appendingPathComponent("AppIcon.icns").path])
