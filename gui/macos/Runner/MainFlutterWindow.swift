import Cocoa
import FlutterMacOS

class MainFlutterWindow: NSWindow {
  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    var windowFrame = self.frame
    self.contentViewController = flutterViewController

    // The layout stacks a device card, the partition table, options, progress
    // and a log pane; the 800x600 default clips the log to a couple of lines.
    windowFrame.size = NSSize(width: 900, height: 760)
    self.setFrame(windowFrame, display: true)
    self.minSize = NSSize(width: 720, height: 600)
    self.title = "Allwinner Flasher"

    RegisterGeneratedPlugins(registry: flutterViewController)

    super.awakeFromNib()
  }
}
