import AppKit

let launch = Launch.parse(CommandLine.arguments)
if let why = launch.refused {
    FileHandle.standardError.write(Data("jaw-mac: \(why)\n".utf8))
    exit(2)
}
let app = NSApplication.shared
let delegate = AppDelegate(launch: launch)
app.delegate = delegate
app.setActivationPolicy(.regular)
app.run()
