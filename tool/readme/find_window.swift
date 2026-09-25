// Prints "<window id> <x> <y> <width> <height>" (points) for the largest
// on-screen window owned by the named app, for screencapture -l / -R.
//
//   swift tool/readme/find_window.swift "Dove Zip"
//
// Owner names and bounds are readable without the Screen Recording
// permission; only window titles would need it, and they aren't used.
//
// From jejezz/application-release-templates common/ @ conventions-v1.

import CoreGraphics
import Foundation

guard CommandLine.arguments.count == 2 else {
  FileHandle.standardError.write("usage: find_window.swift <app name>\n".data(using: .utf8)!)
  exit(2)
}
let owner = CommandLine.arguments[1]

let info = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
  as? [[String: Any]] ?? []

let windows = info.compactMap { w -> (Int, CGRect)? in
  guard (w[kCGWindowOwnerName as String] as? String) == owner,
        (w[kCGWindowLayer as String] as? Int) == 0,
        let id = w[kCGWindowNumber as String] as? Int,
        let boundsDict = w[kCGWindowBounds as String] as? NSDictionary,
        let bounds = CGRect(dictionaryRepresentation: boundsDict)
  else { return nil }
  return (id, bounds)
}

guard let (id, b) = windows.max(by: { $0.1.width * $0.1.height < $1.1.width * $1.1.height }) else {
  let running = Set(info.compactMap { $0[kCGWindowOwnerName as String] as? String }).sorted()
  FileHandle.standardError.write(
    "no on-screen window owned by \"\(owner)\". Running: \(running.joined(separator: ", "))\n"
      .data(using: .utf8)!)
  exit(1)
}
print(id, Int(b.minX), Int(b.minY), Int(b.width), Int(b.height))
