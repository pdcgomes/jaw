import AppKit
import JawKit
import JawThemes
import JawUI
import SwiftUI

/// The harness: each of the app's components in each of its states, drawn from made-up rows and the
/// protocol's fixtures, in either look, dark or light, so a component is built and judged here before
/// it goes into the window, as Redlamp's harness has it. It links the app's own frameworks, and asks
/// no jawd for anything.

@MainActor
final class Harness: NSObject, NSApplicationDelegate {
    var window: NSWindow!
    let stage = NSView()
    var look: Look = .bubbles
    var dark = true
    var density: Density = .comfortable
    var scene = Scene.conversation
    var shown: NSViewController?
    let layouts = Layouts()

    enum Scene: String, CaseIterable {
        case conversation = "Conversation"
        case chatList = "Chat list"
        case states = "States"
        case mimic = "The Mimic"
        case palette = "Palette"
        case roles = "Theme roles"
    }

    var style: Style {
        Style(roles: Catalog.jaw.roles(dark: dark), look: look, density: density)
    }

    func applicationDidFinishLaunching(_: Notification) {
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1200, height: 800),
                          styleMask: [.titled, .closable, .resizable, .miniaturizable], backing: .buffered, defer: false)
        window.title = "Jaw Harness"
        let scenes = NSSegmentedControl(labels: Scene.allCases.map(\.rawValue), trackingMode: .selectOne,
                                        target: self, action: #selector(pickScene(_:)))
        scenes.selectedSegment = 0
        let looks = NSSegmentedControl(labels: ["Bubbles", "Lines"], trackingMode: .selectOne, target: self, action: #selector(pickLook(_:)))
        looks.selectedSegment = 0
        let modes = NSSegmentedControl(labels: ["Dark", "Light"], trackingMode: .selectOne, target: self, action: #selector(pickMode(_:)))
        modes.selectedSegment = 0
        let compact = NSButton(checkboxWithTitle: "Compact", target: self, action: #selector(toggleCompact(_:)))
        let bar = NSStackView(views: [scenes, NSView(), looks, modes, compact])
        bar.edgeInsets = NSEdgeInsets(top: 8, left: 12, bottom: 8, right: 12)
        let root = NSView()
        for v in [bar, stage] {
            v.translatesAutoresizingMaskIntoConstraints = false
            root.addSubview(v)
        }
        NSLayoutConstraint.activate([
            bar.topAnchor.constraint(equalTo: root.topAnchor),
            bar.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            bar.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            stage.topAnchor.constraint(equalTo: bar.bottomAnchor),
            stage.leadingAnchor.constraint(equalTo: root.leadingAnchor),
            stage.trailingAnchor.constraint(equalTo: root.trailingAnchor),
            stage.bottomAnchor.constraint(equalTo: root.bottomAnchor),
        ])
        window.contentView = root
        window.center()
        window.makeKeyAndOrderFront(nil)
        show()
        if let i = CommandLine.arguments.firstIndex(of: "--scene"), i + 1 < CommandLine.arguments.count,
           let s = Scene.allCases.first(where: { $0.rawValue.lowercased().hasPrefix(CommandLine.arguments[i + 1].lowercased()) })
        {
            scene = s
            scenes.selectedSegment = Scene.allCases.firstIndex(of: s)!
            show()
        }
        if CommandLine.arguments.contains("--light") {
            dark = false
            modes.selectedSegment = 1
            show()
        }
        if let i = CommandLine.arguments.firstIndex(of: "--snapshot"), i + 1 < CommandLine.arguments.count {
            let path = CommandLine.arguments[i + 1]
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { [self] in
                let view = window.contentView!
                if let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) {
                    view.cacheDisplay(in: view.bounds, to: rep)
                    try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: path))
                }
                NSApp.terminate(nil)
            }
        }
    }

    @objc func pickScene(_ c: NSSegmentedControl) {
        scene = Scene.allCases[c.selectedSegment]
        show()
    }

    @objc func pickLook(_ c: NSSegmentedControl) {
        look = c.selectedSegment == 0 ? .bubbles : .lines
        show()
    }

    @objc func pickMode(_ c: NSSegmentedControl) {
        dark = c.selectedSegment == 0
        show()
    }

    @objc func toggleCompact(_ b: NSButton) {
        density = b.state == .on ? .compact : .comfortable
        show()
    }

    func show() {
        window.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        shown?.view.removeFromSuperview()
        let vc: NSViewController = switch scene {
        case .conversation: conversation()
        case .chatList: chatList()
        case .states: hosted(States(roles: style.roles))
        case .mimic: hosted(Mimics(roles: style.roles))
        case .palette: palette()
        case .roles: hosted(RolesView(roles: style.roles))
        }
        shown = vc
        vc.view.translatesAutoresizingMaskIntoConstraints = false
        stage.addSubview(vc.view)
        NSLayoutConstraint.activate([
            vc.view.topAnchor.constraint(equalTo: stage.topAnchor),
            vc.view.bottomAnchor.constraint(equalTo: stage.bottomAnchor),
            vc.view.leadingAnchor.constraint(equalTo: stage.leadingAnchor),
            vc.view.trailingAnchor.constraint(equalTo: stage.trailingAnchor),
        ])
    }

    func conversation() -> NSViewController {
        let c = ConversationController(chat: Samples.group, spec: LayoutSpec(style: style, me: ["person:1"]), layouts: layouts,
                                       place: .newest(100))
        _ = c.view
        c.members = [
            ChatInfoMember.make("person:4", "Ana Ribeiro"), ChatInfoMember.make("person:7", "Rui Costa"),
            ChatInfoMember.make("person:9", "Inês Gomes"),
        ]
        DispatchQueue.main.async { c.apply(Samples.update) }
        return c
    }

    func chatList() -> NSViewController {
        let list = ChatListController(folder: "all", roles: style.roles)
        _ = list.view
        let rows = Samples.chats
        list.apply(Update(seq: 1, upsert: rows, total: UInt64(rows.count)))
        list.select(Samples.direct.id)
        return list
    }

    func palette() -> NSViewController {
        let model = PaletteModel()
        model.query = "ar"
        model.answered(id: 1, Self.found)
        return hosted(PaletteView(model: model, roles: style.roles) {}.padding(40))
    }

    /// `find`'s answer from protocol/fixtures/find.result.json, as jawd sends it.
    static var found: Found {
        let json = """
        {"results":[{"kind":"chat","score":62,"text":[{"t":"mark","s":"A"},{"t":"text","s":"na "},{"t":"mark","s":"R"},{"t":"text","s":"ibeiro"}],"field":"name","folders":["Family"],"row":{"id":"351912345678@s.whatsapp.net","v":42,"sort":"1~1792145060000","kind":"direct","name":"Ana Ribeiro","avatar":null,"last":null,"unread":2,"mentions":0,"pinned":true,"muted_until":null,"archived":false,"marked_unread":false,"draft":null,"community":null}},{"kind":"person","score":62,"text":[{"t":"mark","s":"Ar"},{"t":"text","s":"tur Reis"}],"field":"name","row":{"id":"person:31","name":"Artur Reis","phone":"+351913000031"}},{"kind":"message","score":0,"text":[{"t":"text","s":"vamos à "},{"t":"mark","s":"Arrábida"},{"t":"text","s":" sábado?"}],"chat_name":"Lisbon Climbing","row":{"id":"3EB0C431F2A9B8E7D6","v":81200,"sort":"1792050000000~000412","chat":"120363000000000001@g.us","at":1792050000000,"from":{"id":12,"name":"Rui"},"mine":false,"kind":"text","spans":[{"t":"text","s":"vamos à Arrábida sábado?"}]}}],"counts":{"chat":1,"person":2,"message":214}}
        """
        return (try? JSONDecoder().decode(Found.self, from: Data(json.utf8))) ?? Found()
    }

    func hosted(_ view: some View) -> NSViewController {
        let host = NSHostingController(rootView: AnyView(view.focusEffectDisabled()))
        host.sizingOptions = []
        return host
    }
}

enum ChatInfoMember {
    static func make(_ person: String, _ name: String) -> ChatInfo.Member {
        let json = "{\"person\":\"\(person)\",\"name\":\"\(name)\",\"admin\":false}"
        return try! JSONDecoder().decode(ChatInfo.Member.self, from: Data(json.utf8))
    }
}

struct States: View {
    let roles: Roles

    var body: some View {
        ScrollView {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 360), spacing: 12)], spacing: 12) {
                ForEach(Array(kinds.enumerated()), id: \.offset) { _, kind in
                    StateView(kind: kind, roles: roles).frame(height: 360).clipShape(RoundedRectangle(cornerRadius: Radius.field))
                }
                VStack {
                    EmptyChatView(chat: Samples.group).frame(height: 300)
                    StatusLine(state: "catching up 1,204 of 5,000", warn: false, chat: "someone's typing…", roles: roles)
                    StatusLine(state: "phone offline: photos and older history wait for it", warn: true, chat: nil, roles: roles)
                }
            }
            .padding(12)
        }
        .background(Color(nsColor: roles.ground.nsColor))
    }

    var kinds: [StateView.Kind] {
        [.noChat, .noChats, .starting(failed: nil), .starting(failed: "jawd stopped as it started (status 1)"),
         .asleep(why: "jawd isn't running: nothing answers on ~/Library/Application Support/jaw/jaw.sock", retryIn: 2),
         .loggedOut(reason: "logged out from the phone"), .unlinked]
    }
}

struct Mimics: View {
    let roles: Roles
    @State var chomps = 0

    var body: some View {
        VStack(spacing: 30) {
            HStack(spacing: 40) {
                VStack { MimicImage(look: .awake, scale: 4, chomps: chomps); Text("awake") }
                VStack { MimicImage(look: .chomp, scale: 4); Text("mid-chomp") }
                VStack { MimicImage(look: .asleep, scale: 4); Text("asleep") }
            }
            HStack(spacing: 40) {
                MimicImage(look: .awake, scale: 1)
                MimicImage(look: .awake, scale: 2)
                MimicImage(look: .awake, scale: 8)
            }
            Button("Chomp") { chomps += 1 }
        }
        .foregroundStyle(Color(nsColor: roles.secondary.nsColor))
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(nsColor: roles.ground.nsColor))
    }
}

struct RolesView: View {
    let roles: Roles

    var body: some View {
        let named: [(String, RGBA)] = [
            ("ground", roles.ground), ("surface", roles.surface), ("selection", roles.selection), ("text", roles.text),
            ("secondary", roles.secondary), ("tertiary", roles.tertiary), ("separator", roles.separator),
            ("accent", roles.accent), ("yours", roles.yours), ("mention", roles.mention), ("online", roles.online),
            ("failed", roles.failed),
        ] + roles.senders.enumerated().map { ("sender \($0.offset)", $0.element) }
        return ScrollView {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 140))], spacing: 10) {
                ForEach(named, id: \.0) { name, colour in
                    VStack(alignment: .leading) {
                        RoundedRectangle(cornerRadius: Radius.control).fill(colour.color).frame(height: 50)
                            .overlay(Text("Aa").foregroundStyle(Color(nsColor: roles.text.nsColor)))
                        Text(name).textStyle(.caption)
                        Text(String(format: "%.1f:1 with text", roles.text.contrast(with: colour))).textStyle(.micro).foregroundStyle(.secondary)
                    }
                }
            }
            .padding(16)
        }
        .background(Color(nsColor: roles.ground.nsColor))
    }
}

let app = NSApplication.shared
let harness = Harness()
app.delegate = harness
app.setActivationPolicy(.regular)
app.run()
