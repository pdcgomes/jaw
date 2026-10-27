import AppKit
import JawThemes
import JawUI
import SwiftUI

/// The bar along the bottom of the window, where the controls live so the panes have the window's
/// whole height: the sidebar, the layout and the splits on the left, the status line between, and
/// Find and New Chat on the right. Each runs the same command as its menu item and key.
@MainActor
final class BottomBar: NSView {
    let status: NSHostingView<StatusLine>
    let sidebar: IconButton
    let layout: IconButton
    let split: IconButton
    let find: IconButton
    let newChat: IconButton
    let rule = NSView()
    static let height: CGFloat = 30

    init(model: AppModel) {
        status = NSHostingView(rootView: StatusLine(state: nil, warn: false, chat: nil, roles: model.roles))
        let run = { [weak model] (id: String) in model?.command(id)?.run() }
        sidebar = IconButton(symbol: "sidebar.left", label: "Show or fold the sidebar", shortcut: Shortcut("b")) { run("sidebar") }
        layout = IconButton(symbol: "square.grid.3x2", label: "Layouts on a grid", shortcut: Shortcut("e", [.command, .option]),
                            title: "Unsaved Panes") {}
        split = IconButton(symbol: "rectangle.split.2x1", label: "Split this pane", shortcut: Shortcut("d")) {}
        find = IconButton(symbol: "magnifyingglass", label: "Find: chats, people, messages and every action", shortcut: Shortcut("k"),
                          title: "Find") { run("palette") }
        newChat = IconButton(symbol: "square.and.pencil", label: "Start a chat", shortcut: Shortcut("n")) { run("chat.new") }
        super.init(frame: .zero)
        wantsLayer = true
        layout.menuToOpen = { [weak model] in
            model.map { m in Menus.menu(m.allCommands.filter { $0.place == .layouts }) }
        }
        split.menuToOpen = { [weak model] in
            model.map { m in Menus.menu(["split.across", "split.down", "pane.close"].compactMap(m.command)) }
        }
        let left = NSStackView(views: [sidebar, layout, split])
        let right = NSStackView(views: [find, newChat])
        for stack in [left, right] {
            stack.orientation = .horizontal
            stack.spacing = Space.xxs
        }
        rule.wantsLayer = true
        for v in [rule, left, status, right] as [NSView] {
            v.translatesAutoresizingMaskIntoConstraints = false
            addSubview(v)
        }
        // Stretched between the controls, not held to its own width.
        status.sizingOptions = []
        NSLayoutConstraint.activate([
            heightAnchor.constraint(equalToConstant: Self.height),
            rule.topAnchor.constraint(equalTo: topAnchor),
            rule.leadingAnchor.constraint(equalTo: leadingAnchor),
            rule.trailingAnchor.constraint(equalTo: trailingAnchor),
            rule.heightAnchor.constraint(equalToConstant: Stroke.hairline),
            left.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Space.s),
            left.centerYAnchor.constraint(equalTo: centerYAnchor),
            status.leadingAnchor.constraint(equalTo: left.trailingAnchor, constant: Space.s),
            status.trailingAnchor.constraint(equalTo: right.leadingAnchor, constant: -Space.s),
            status.topAnchor.constraint(equalTo: topAnchor, constant: Stroke.hairline),
            status.bottomAnchor.constraint(equalTo: bottomAnchor),
            right.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Space.s),
            right.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func restyle(_ roles: Roles) {
        layer?.backgroundColor = roles.surface.cgColor
        rule.layer?.backgroundColor = roles.separator.mixed(with: roles.surface, 0.5).cgColor
        for b in [sidebar, layout, split, find, newChat] {
            b.roles = roles
        }
    }

    /// The layout on screen, by name, with • when its shape has changed since it was saved.
    func show(layout name: String, edited: Bool) {
        layout.title = edited ? "\(name) •" : name
        layout.toolTip = edited
            ? "\(name), changed since it was saved: Save Layout (⌥⌘S) keeps it"
            : "Layouts on a grid: Edit Layout (⌥⌘E), and ⌃⌘1 to ⌃⌘9 to go to one"
    }
}
