// Render an SVG to a transparent PNG with macOS's own SVG renderer
// (conventions/icons.md §1). ImageMagick's built-in renderer drops strokes
// and misplaces shapes in Icons8 SVGs, so generate_icons.py calls this.
//
//     swift tool/icon/render_svg.swift in.svg out.png [size]
//
// From jejezz/application-release-templates common/ @ conventions-v1.

import AppKit

let args = CommandLine.arguments
guard args.count >= 3 else {
    FileHandle.standardError.write("usage: render_svg.swift in.svg out.png [size]\n".data(using: .utf8)!)
    exit(2)
}
let size = args.count > 3 ? Int(args[3]) ?? 1024 : 1024
guard let image = NSImage(contentsOf: URL(fileURLWithPath: args[1])) else {
    FileHandle.standardError.write("cannot read \(args[1])\n".data(using: .utf8)!)
    exit(1)
}
let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size,
                           bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                           colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
NSGraphicsContext.current?.imageInterpolation = .high
image.draw(in: NSRect(x: 0, y: 0, width: size, height: size))
NSGraphicsContext.restoreGraphicsState()
try rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: args[2]))
