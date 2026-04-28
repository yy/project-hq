#!/usr/bin/env swift

import AppKit
import Foundation

let root = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
let assetDir = root.appendingPathComponent("macos/Assets", isDirectory: true)
let sourceURL = assetDir.appendingPathComponent("AppIcon-1024.png")
let iconsetURL = assetDir.appendingPathComponent("AppIcon.iconset", isDirectory: true)
let icnsURL = assetDir.appendingPathComponent("AppIcon.icns")

try FileManager.default.createDirectory(at: assetDir, withIntermediateDirectories: true)
try? FileManager.default.removeItem(at: iconsetURL)
try FileManager.default.createDirectory(at: iconsetURL, withIntermediateDirectories: true)

let size = CGSize(width: 1024, height: 1024)
let image = NSImage(size: size)

image.lockFocus()

let bodySize: CGFloat = 824
let bodyInset: CGFloat = (size.width - bodySize) / 2
let bodyScale = bodySize / size.width
let transform = NSAffineTransform()
transform.translateX(by: bodyInset, yBy: bodyInset)
transform.scaleX(by: bodyScale, yBy: bodyScale)
transform.concat()

let canvas = NSRect(origin: .zero, size: size)
let outer = NSBezierPath(roundedRect: canvas, xRadius: 224, yRadius: 224)
NSColor(red: 0.02, green: 0.04, blue: 0.06, alpha: 1).setFill()
outer.fill()

let inner = NSBezierPath(roundedRect: canvas.insetBy(dx: 34, dy: 34), xRadius: 190, yRadius: 190)
NSColor(red: 0.02, green: 0.10, blue: 0.18, alpha: 1).setFill()
inner.fill()

let paragraph = NSMutableParagraphStyle()
paragraph.alignment = .center
let shadowAttributes: [NSAttributedString.Key: Any] = [
    .font: NSFont.systemFont(ofSize: 410, weight: .black),
    .foregroundColor: NSColor(red: 0.00, green: 0.00, blue: 0.00, alpha: 0.28),
    .kern: -24,
    .paragraphStyle: paragraph,
]
let textAttributes: [NSAttributedString.Key: Any] = [
    .font: NSFont.systemFont(ofSize: 410, weight: .black),
    .foregroundColor: NSColor.white,
    .kern: -24,
    .paragraphStyle: paragraph,
]
"HQ".draw(in: NSRect(x: 34, y: 305, width: 956, height: 460).offsetBy(dx: 0, dy: -16), withAttributes: shadowAttributes)
"HQ".draw(in: NSRect(x: 34, y: 305, width: 956, height: 460), withAttributes: textAttributes)

let accent = NSBezierPath(roundedRect: NSRect(x: 244, y: 236, width: 536, height: 36), xRadius: 18, yRadius: 18)
NSColor(red: 0.00, green: 0.86, blue: 0.78, alpha: 1).setFill()
accent.fill()

image.unlockFocus()

guard let tiff = image.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiff),
      let png = bitmap.representation(using: .png, properties: [:])
else {
    fatalError("Could not render icon PNG")
}
try png.write(to: sourceURL)

func resizeIcon(size points: Int, scale: Int) throws {
    let pixels = points * scale
    let outputName = scale == 1
        ? "icon_\(points)x\(points).png"
        : "icon_\(points)x\(points)@\(scale)x.png"
    let outputURL = iconsetURL.appendingPathComponent(outputName)
    let task = Process()
    task.executableURL = URL(fileURLWithPath: "/usr/bin/sips")
    task.arguments = ["-z", "\(pixels)", "\(pixels)", sourceURL.path, "--out", outputURL.path]
    try task.run()
    task.waitUntilExit()
    if task.terminationStatus != 0 {
        fatalError("sips failed while creating \(outputName)")
    }
}

for points in [16, 32, 128, 256, 512] {
    try resizeIcon(size: points, scale: 1)
    try resizeIcon(size: points, scale: 2)
}

let task = Process()
task.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
task.arguments = ["-c", "icns", iconsetURL.path, "-o", icnsURL.path]
try task.run()
task.waitUntilExit()
if task.terminationStatus != 0 {
    fatalError("iconutil failed")
}

print(icnsURL.path)
