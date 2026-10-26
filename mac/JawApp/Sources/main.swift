import AppKit

let launch = Launch.parse(CommandLine.arguments)
let app = NSApplication.shared
let delegate = AppDelegate(launch: launch)
app.delegate = delegate
app.setActivationPolicy(.regular)
app.run()
