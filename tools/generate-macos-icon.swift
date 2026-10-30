// Recreate packaging/macos/PteExplorer.icns on macOS:
// swift tools/generate-macos-icon.swift /tmp/PteExplorer.iconset
// iconutil -c icns /tmp/PteExplorer.iconset -o packaging/macos/PteExplorer.icns
import AppKit

let output = URL(fileURLWithPath: CommandLine.arguments[1])
try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
func color(_ hex: Int) -> NSColor {
    NSColor(srgbRed: CGFloat((hex >> 16) & 255) / 255,
            green: CGFloat((hex >> 8) & 255) / 255,
            blue: CGFloat(hex & 255) / 255, alpha: 1)
}
for points in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let pixels = points * scale
        let image = NSImage(size: NSSize(width: pixels, height: pixels))
        image.lockFocus()
        let transform = NSAffineTransform()
        transform.scale(by: CGFloat(pixels) / 1024)
        transform.concat()
        let tile = NSBezierPath(roundedRect: NSRect(x: 72, y: 72, width: 880, height: 880),
                                xRadius: 196, yRadius: 196)
        NSGradient(starting: color(0x243c58), ending: color(0x111822))!.draw(in: tile, angle: -90)
        // A branching operator graph echoes the explorer's graph canvas.
        let links = NSBezierPath()
        links.move(to: NSPoint(x: 306, y: 512))
        links.line(to: NSPoint(x: 480, y: 512))
        links.line(to: NSPoint(x: 480, y: 700))
        links.line(to: NSPoint(x: 684, y: 700))
        links.move(to: NSPoint(x: 480, y: 512))
        links.line(to: NSPoint(x: 480, y: 324))
        links.line(to: NSPoint(x: 684, y: 324))
        color(0xb1c9df).setStroke()
        links.lineWidth = 38
        links.lineJoinStyle = .round
        links.stroke()
        for (x, y, fill) in [(220, 426, 0x60d4c5), (612, 628, 0x79b8ff), (612, 252, 0x79b8ff)] {
            color(fill).setFill()
            NSBezierPath(roundedRect: NSRect(x: x, y: y, width: 172, height: 172),
                         xRadius: 38, yRadius: 38).fill()
        }
        image.unlockFocus()
        let bitmap = NSBitmapImageRep(data: image.tiffRepresentation!)!
        let suffix = scale == 2 ? "@2x" : ""
        try bitmap.representation(using: .png, properties: [:])!
            .write(to: output.appendingPathComponent("icon_\(points)x\(points)\(suffix).png"))
    }
}
